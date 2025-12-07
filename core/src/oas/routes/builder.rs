#![deny(missing_docs)]

//! # Route Builder
//!
//! Logic that transforms `Shim` structs into `ParsedRoute` IR models.

use crate::error::AppResult;
use crate::oas::models::{
    ParamSource, ParsedRoute, RouteKind, RouteParam, SecurityRequirement, SecuritySchemeInfo,
    SecuritySchemeKind,
};
use crate::oas::resolver::responses::extract_response_details;
use crate::oas::resolver::{extract_request_body_type, resolve_parameters};
use crate::oas::routes::callbacks::{extract_callback_operations, resolve_callback_object};
use crate::oas::routes::naming::{derive_handler_name, to_snake_case};
use crate::oas::routes::shims::{ShimComponents, ShimOperation, ShimPathItem, ShimSecurityScheme};
use crate::parser::ParsedExternalDocs;
use std::collections::HashMap;
use utoipa::openapi::RefOr;
// ... (Rest of file logic remains largely identical, included for completeness)

/// Helper to iterate methods in a ShimPathItem and extract all operations as Routes.
pub fn parse_path_item(
    routes: &mut Vec<ParsedRoute>,
    path_or_name: &str,
    path_item: ShimPathItem,
    kind: RouteKind,
    components: Option<&ShimComponents>,
    base_path: Option<String>,
) -> AppResult<()> {
    // Handle common parameters defined at PathItem level.
    let common_params_list = path_item.parameters.as_deref().unwrap_or(&[]);
    let common_params = resolve_parameters(common_params_list, components)?;

    let mut add_op = |method: &str, op: Option<ShimOperation>| -> AppResult<()> {
        if let Some(o) = op {
            routes.push(build_route(
                path_or_name,
                method,
                o,
                &common_params,
                kind.clone(),
                components,
                base_path.clone(),
            )?);
        }
        Ok(())
    };

    add_op("GET", path_item.get)?;
    add_op("POST", path_item.post)?;
    add_op("PUT", path_item.put)?;
    add_op("DELETE", path_item.delete)?;
    add_op("PATCH", path_item.patch)?;
    add_op("OPTIONS", path_item.options)?;
    add_op("HEAD", path_item.head)?;
    add_op("TRACE", path_item.trace)?;
    add_op("QUERY", path_item.query)?;

    Ok(())
}

fn build_route(
    path: &str,
    method: &str,
    op: ShimOperation,
    common_params: &[RouteParam],
    kind: RouteKind,
    components: Option<&ShimComponents>,
    base_path: Option<String>,
) -> AppResult<ParsedRoute> {
    // 1. Handler Name
    let handler_name = if let Some(op_id) = &op.operation_id {
        to_snake_case(op_id)
    } else {
        derive_handler_name(method, path)
    };

    // 2. Parameters
    let op_params_list = op.parameters.as_deref().unwrap_or(&[]);
    let op_params = resolve_parameters(op_params_list, components)?;

    let mut params = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Operation params take precedence over Common params
    for p in op_params {
        seen.insert((p.name.clone(), p.source.clone()));
        params.push(p);
    }
    for p in common_params {
        if !seen.contains(&(p.name.clone(), p.source.clone())) {
            params.push(p.clone());
        }
    }

    // 3. Request Body
    let mut request_body = None;
    if let Some(req_body_ref) = &op.request_body {
        if let Some(def) = extract_request_body_type(req_body_ref)? {
            request_body = Some(def);
        }
    }

    // 4. Security
    let mut security = Vec::new();
    if let Some(requirements) = &op.security {
        for req in requirements {
            if let Ok(map) = serde_json::from_value::<HashMap<String, Vec<String>>>(req.clone()) {
                for (scheme_name, scopes) in map {
                    // Resolve Scheme Details
                    let scheme = resolve_security_scheme(&scheme_name, components);

                    security.push(SecurityRequirement {
                        scheme_name,
                        scopes,
                        scheme,
                    });
                }
            }
        }
    }

    // 5. Response Type, Headers, and Links
    let (response_type, response_headers, response_links) =
        if let Some(details) = extract_response_details(&op.responses, components)? {
            (details.body_type, details.headers, details.links)
        } else {
            (None, Vec::new(), Vec::new())
        };

    // 6. Callbacks
    let mut parsed_callbacks = Vec::new();
    if let Some(cb_map) = &op.callbacks {
        for (cb_name, cb_ref) in cb_map {
            // Resolve RefOr -> BTreeMap<Expression, PathItem>
            let callback_defs = resolve_callback_object(cb_ref, components)?;

            for (expression, path_item) in callback_defs {
                // Flatten the PathItem inside the callback into ParsedCallback entries
                extract_callback_operations(
                    &mut parsed_callbacks,
                    cb_name,
                    &expression,
                    &path_item,
                    components,
                )?;
            }
        }
    }

    // 7. Metadata (Deprecated & External Docs)
    let external_docs = op.external_docs.map(|d| ParsedExternalDocs {
        url: d.url,
        description: d.description,
    });

    Ok(ParsedRoute {
        path: path.to_string(),
        base_path,
        method: method.to_string(),
        handler_name,
        params,
        request_body,
        security,
        response_type,
        response_headers,
        response_links: Some(response_links),
        kind,
        callbacks: parsed_callbacks,
        deprecated: op.deprecated,
        external_docs,
    })
}

fn resolve_security_scheme(
    name: &str,
    components: Option<&ShimComponents>,
) -> Option<SecuritySchemeInfo> {
    let comps = components?;
    let schemes = comps.security_schemes.as_ref()?;

    if let Some(RefOr::T(shim)) = schemes.get(name) {
        let kind = match shim {
            ShimSecurityScheme::ApiKey(k) => {
                let source = match k.in_loc.as_str() {
                    "header" => ParamSource::Header,
                    "query" => ParamSource::Query,
                    "cookie" => ParamSource::Cookie,
                    _ => ParamSource::Query,
                };
                SecuritySchemeKind::ApiKey {
                    name: k.name.clone(),
                    in_loc: source,
                }
            }
            ShimSecurityScheme::Http(h) => SecuritySchemeKind::Http {
                scheme: h.scheme.clone(),
                bearer_format: h.bearer_format.clone(),
            },
            ShimSecurityScheme::OAuth2(_) => SecuritySchemeKind::OAuth2,
            ShimSecurityScheme::OpenIdConnect(_) => SecuritySchemeKind::OpenIdConnect,
            ShimSecurityScheme::MutualTls(_) => SecuritySchemeKind::MutualTls,
            ShimSecurityScheme::Basic => SecuritySchemeKind::Http {
                scheme: "basic".into(),
                bearer_format: None,
            },
        };

        let description = match shim {
            ShimSecurityScheme::ApiKey(k) => k.description.clone(),
            ShimSecurityScheme::Http(h) => h.description.clone(),
            ShimSecurityScheme::OAuth2(o) => o.description.clone(),
            ShimSecurityScheme::OpenIdConnect(o) => o.description.clone(),
            ShimSecurityScheme::MutualTls(m) => m.description.clone(),
            ShimSecurityScheme::Basic => None,
        };

        Some(SecuritySchemeInfo { kind, description })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::oas::models::RuntimeExpression;

    #[test]
    fn test_parse_callback_inline() {
        let yaml = r#"
openapi: 3.1.0
info: {title: Callback Test, version: 1.0}
paths:
  /subscribe:
    post:
      responses: { '200': {description: OK} }
      callbacks:
        onData:
          '{$request.body#/url}':
            post:
              requestBody:
                content: { application/json: { schema: {type: object} } }
              responses: { '200': {description: OK} }
"#;
        let routes = super::super::parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        let route = &routes[0];

        assert_eq!(route.callbacks.len(), 1);
        let cb = &route.callbacks[0];
        assert_eq!(cb.name, "onData");
        // Test strict expression type checking
        assert_eq!(
            cb.expression,
            RuntimeExpression::new("{$request.body#/url}")
        );
        assert_eq!(cb.method, "POST");
        assert!(cb.request_body.is_some());
    }
}
