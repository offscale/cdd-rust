#![deny(missing_docs)]

//! # Route Builder
//!
//! Logic that transforms `Shim` structs into `ParsedRoute` IR models.

use crate::error::{AppError, AppResult};
use crate::oas::models::{
    ParamSource, ParsedRoute, RouteKind, RouteParam, SecurityRequirement, SecuritySchemeInfo,
    SecuritySchemeKind,
};
use crate::oas::ref_utils::extract_component_name;
use crate::oas::resolver::responses::extract_response_details;
use crate::oas::resolver::{extract_request_body_type, resolve_parameters};
use crate::oas::routes::callbacks::{extract_callback_operations, resolve_callback_object};
use crate::oas::routes::naming::{derive_handler_name, to_snake_case};
use crate::oas::routes::shims::{ShimComponents, ShimOperation, ShimPathItem, ShimSecurityScheme};
use crate::parser::ParsedExternalDocs;
use regex::Regex;
use std::collections::HashMap;
use utoipa::openapi::RefOr;

/// Helper to iterate methods in a ShimPathItem and extract all operations as Routes.
pub fn parse_path_item(
    routes: &mut Vec<ParsedRoute>,
    path_or_name: &str,
    path_item: &ShimPathItem,
    kind: RouteKind,
    components: Option<&ShimComponents>,
    is_oas3: bool,
    base_path: Option<String>,
    global_security: Option<&Vec<serde_json::Value>>,
    operation_index: &mut HashMap<String, String>,
    operation_ids: &mut std::collections::HashSet<String>,
    all_paths: &std::collections::BTreeMap<String, ShimPathItem>,
) -> AppResult<()> {
    let mut path_item = path_item.clone();
    if let Some(ref_path) = path_item.ref_path.clone() {
        path_item = super::resolve_path_item_ref(&ref_path, components, all_paths)?;
    }

    let path_summary = path_item.summary.clone();
    let path_description = path_item.description.clone();
    let path_base_path =
        super::resolve_base_path_from_servers(path_item.servers.as_ref()).or(base_path.clone());

    // Handle common parameters defined at PathItem level.
    let common_params_list = path_item.parameters.as_deref().unwrap_or(&[]);
    let common_params = resolve_parameters(common_params_list, components, is_oas3)?;

    let mut add_op = |method: &str, op: Option<ShimOperation>| -> AppResult<()> {
        if let Some(o) = op {
            if let Some(op_id) = &o.operation_id {
                if !operation_ids.insert(op_id.clone()) {
                    return Err(AppError::General(format!(
                        "Duplicate operationId '{}' detected",
                        op_id
                    )));
                }
                operation_index.insert(op_id.clone(), path_or_name.to_string());
            }
            routes.push(build_route(
                path_or_name,
                method,
                o,
                &common_params,
                kind.clone(),
                components,
                is_oas3,
                path_base_path.clone(),
                path_summary.clone(),
                path_description.clone(),
                global_security,
                operation_ids,
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

    if let Some(additional) = &path_item.additional_operations {
        for method in additional.keys() {
            if !is_http_token(method) {
                return Err(AppError::General(format!(
                    "additionalOperations method '{}' must be a valid HTTP token",
                    method
                )));
            }
            if is_reserved_method(method) {
                return Err(AppError::General(format!(
                    "additionalOperations contains reserved HTTP method '{}'",
                    method
                )));
            }
        }
        for (method, op) in additional {
            if is_reserved_method(method) {
                continue;
            }
            add_op(&method.to_ascii_uppercase(), Some(op.clone()))?;
        }
    }

    Ok(())
}

fn build_route(
    path: &str,
    method: &str,
    op: ShimOperation,
    common_params: &[RouteParam],
    kind: RouteKind,
    components: Option<&ShimComponents>,
    is_oas3: bool,
    path_base_path: Option<String>,
    path_summary: Option<String>,
    path_description: Option<String>,
    global_security: Option<&Vec<serde_json::Value>>,
    operation_ids: &mut std::collections::HashSet<String>,
) -> AppResult<ParsedRoute> {
    let base_path = super::resolve_base_path_from_servers(op.servers.as_ref()).or(path_base_path);
    // 1. Handler Name
    let handler_name = if let Some(op_id) = &op.operation_id {
        to_snake_case(op_id)
    } else {
        derive_handler_name(method, path)
    };

    // 2. Parameters
    let op_params_list = op.parameters.as_deref().unwrap_or(&[]);
    let op_params = resolve_parameters(op_params_list, components, is_oas3)?;

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

    let querystring_count = params
        .iter()
        .filter(|p| p.source == ParamSource::QueryString)
        .count();
    if querystring_count > 1 {
        return Err(AppError::General(format!(
            "Operation '{}' defines multiple querystring parameters",
            handler_name
        )));
    }
    if querystring_count == 1 && params.iter().any(|p| p.source == ParamSource::Query) {
        return Err(AppError::General(format!(
            "Operation '{}' mixes 'querystring' and 'query' parameters",
            handler_name
        )));
    }

    if matches!(kind, RouteKind::Path) {
        validate_path_parameters(path, &params)?;
    }

    // 3. Request Body
    let mut request_body = None;
    if let Some(req_body_ref) = &op.request_body {
        if let Some(def) = extract_request_body_type(req_body_ref, components)? {
            request_body = Some(def);
        }
    }

    // 4. Security
    // Operation-level security overrides global security. An explicit empty array clears security.
    let security = if let Some(requirements) = &op.security {
        if requirements.is_empty() {
            Vec::new()
        } else {
            parse_security_requirements(requirements, components)
        }
    } else if let Some(global) = global_security {
        parse_security_requirements(global, components)
    } else {
        Vec::new()
    };

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
                    operation_ids,
                )?;
            }
        }
    }

    // 7. Metadata (Deprecated & External Docs)
    let external_docs = op.external_docs.map(|d| ParsedExternalDocs {
        url: d.url,
        description: d.description,
    });

    let summary = op.summary.clone().or(path_summary);
    let description = op.description.clone().or(path_description);

    // 8. Tags (Used for module grouping)
    let tags = op.tags.unwrap_or_default();

    Ok(ParsedRoute {
        path: path.to_string(),
        summary,
        description,
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
        tags,
        callbacks: parsed_callbacks,
        deprecated: op.deprecated,
        external_docs,
    })
}

fn validate_path_parameters(path: &str, params: &[RouteParam]) -> AppResult<()> {
    let re = Regex::new(r"\{([^}]+)}").expect("Invalid regex constant");
    let mut path_vars = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for cap in re.captures_iter(path) {
        let name = cap[1].to_string();
        if !seen.insert(name.clone()) {
            return Err(AppError::General(format!(
                "Path template '{}' contains duplicate path parameter '{}'",
                path, name
            )));
        }
        path_vars.push(name);
    }

    let path_set: std::collections::HashSet<_> = path_vars.iter().cloned().collect();

    for name in &path_vars {
        if !params
            .iter()
            .any(|p| p.source == ParamSource::Path && p.name == *name)
        {
            return Err(AppError::General(format!(
                "Path template '{}' is missing path parameter definition for '{}'",
                path, name
            )));
        }
    }

    for param in params.iter().filter(|p| p.source == ParamSource::Path) {
        if !path_set.contains(&param.name) {
            return Err(AppError::General(format!(
                "Path parameter '{}' is not present in path template '{}'",
                param.name, path
            )));
        }
    }

    Ok(())
}

fn is_reserved_method(method: &str) -> bool {
    matches!(
        method.to_ascii_lowercase().as_str(),
        "get" | "post" | "put" | "delete" | "patch" | "options" | "head" | "trace" | "query"
    )
}

fn is_http_token(method: &str) -> bool {
    !method.is_empty() && method.chars().all(is_tchar)
}

fn is_tchar(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '!' | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '.'
                | '^'
                | '_'
                | '`'
                | '|'
                | '~'
        )
}

#[cfg(test)]
mod additional_tests {
    use super::*;
    use crate::oas::routes::shims::{ShimOperation, ShimPathItem};
    use std::collections::BTreeMap;
    use utoipa::openapi::Responses;

    fn empty_operation() -> ShimOperation {
        ShimOperation {
            operation_id: None,
            summary: None,
            description: None,
            parameters: None,
            request_body: None,
            responses: crate::oas::routes::shims::ShimResponses::from(Responses::new()),
            security: None,
            callbacks: None,
            tags: None,
            deprecated: false,
            external_docs: None,
            servers: None,
            extensions: BTreeMap::new(),
        }
    }

    fn empty_path_item() -> ShimPathItem {
        ShimPathItem {
            ref_path: None,
            summary: None,
            description: None,
            servers: None,
            parameters: None,
            get: None,
            post: None,
            put: None,
            delete: None,
            patch: None,
            options: None,
            head: None,
            trace: None,
            query: None,
            additional_operations: None,
            extensions: BTreeMap::new(),
        }
    }

    #[test]
    fn test_additional_operations_invalid_method_token_rejected() {
        let mut path_item = empty_path_item();
        let mut additional = BTreeMap::new();
        additional.insert("BAD METHOD".to_string(), empty_operation());
        path_item.additional_operations = Some(additional);

        let mut routes = Vec::new();
        let mut operation_index = HashMap::new();
        let mut operation_ids = std::collections::HashSet::new();
        let all_paths = BTreeMap::new();

        let err = parse_path_item(
            &mut routes,
            "/copy",
            &path_item,
            RouteKind::Path,
            None,
            true,
            None,
            None,
            &mut operation_index,
            &mut operation_ids,
            &all_paths,
        )
        .unwrap_err();

        assert!(format!("{err}").contains("must be a valid HTTP token"));
    }
}

/// Parses a list of Security Requirement Objects into internal requirements.
///
/// Empty requirement objects (`{}`) are treated as "no security required" and skipped.
fn parse_security_requirements(
    requirements: &[serde_json::Value],
    components: Option<&ShimComponents>,
) -> Vec<SecurityRequirement> {
    let mut security = Vec::new();
    for req in requirements {
        if let Ok(map) = serde_json::from_value::<HashMap<String, Vec<String>>>(req.clone()) {
            if map.is_empty() {
                continue;
            }
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
    security
}

fn resolve_security_scheme(
    name: &str,
    components: Option<&ShimComponents>,
) -> Option<SecuritySchemeInfo> {
    let comps = components?;
    let schemes = comps.security_schemes.as_ref()?;

    if let Some(RefOr::T(shim)) = schemes.get(name) {
        return Some(build_security_scheme_info(shim));
    }

    // URI-form security scheme reference (OAS 3.2).
    let self_uri = comps.extra.get("__self").and_then(|v| v.as_str());
    let ref_name = extract_component_name(name, self_uri, "securitySchemes")?;
    if let Some(RefOr::T(shim)) = schemes.get(&ref_name) {
        return Some(build_security_scheme_info(shim));
    }

    None
}

fn build_security_scheme_info(shim: &ShimSecurityScheme) -> SecuritySchemeInfo {
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

    SecuritySchemeInfo { kind, description }
}

#[cfg(test)]
mod tests {
    use crate::oas::models::RuntimeExpression;
    use crate::oas::models::SecuritySchemeKind;

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

    #[test]
    fn test_security_scheme_uri_reference_resolution() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Security, version: 1.0}
components:
  securitySchemes:
    ApiKeyAuth:
      type: apiKey
      name: api-key
      in: header
paths:
  /secure:
    get:
      security:
        - '#/components/securitySchemes/ApiKeyAuth': []
      responses:
        '200': {description: ok}
"#;
        let routes = super::super::parse_openapi_routes(yaml).unwrap();
        let scheme = routes[0]
            .security
            .first()
            .and_then(|s| s.scheme.clone())
            .expect("expected resolved scheme");

        assert!(matches!(scheme.kind, SecuritySchemeKind::ApiKey { .. }));
    }
}
