#![deny(missing_docs)]

//! # Route Builder
//!
//! Logic that transforms `Shim` structs into `ParsedRoute` IR models.

use crate::classes::parse::ParsedExternalDocs;
use crate::error::{AppError, AppResult};
use crate::openapi::parse::models::{
    OAuthFlow, OAuthFlows, ParamSource, ParsedRoute, ParsedServer, ParsedServerVariable, RouteKind,
    RouteParam, SecurityRequirement, SecuritySchemeInfo, SecuritySchemeKind,
};
use crate::openapi::parse::ref_utils::extract_component_name;
use crate::openapi::parse::registry::DocumentRegistry;
use crate::openapi::parse::resolver::body::{
    extract_request_body_raw_with_registry, extract_request_body_type_with_registry,
};
use crate::openapi::parse::resolver::resolve_parameters_with_registry;
use crate::openapi::parse::resolver::responses::extract_response_details_with_registry;
use crate::openapi::parse::routes::callbacks::{
    extract_callback_operations, resolve_callback_object,
};
use crate::openapi::parse::routes::naming::{derive_handler_name, to_snake_case};
use crate::openapi::parse::routes::shims::{
    ShimComponents, ShimOAuth2, ShimOAuthFlow, ShimOperation, ShimPathItem, ShimSecurityScheme,
    ShimServer, ShimServerVariable,
};
use regex::Regex;
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, HashMap};
use url::Url;
use utoipa::openapi::RefOr;

/// Helper to iterate methods in a ShimPathItem and extract all operations as Routes.
#[allow(clippy::too_many_arguments)]
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
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<()> {
    let mut path_item = path_item.clone();
    let mut override_components: Option<ShimComponents> = None;
    let mut override_base: Option<Url> = None;
    if let Some(ref_path) = path_item.ref_path.clone() {
        let (resolved, comps_override, base_override) = super::resolve_path_item_ref_with_context(
            &ref_path, components, all_paths, registry, base_uri,
        )?;
        path_item = resolved;
        override_components = comps_override;
        override_base = base_override;
    }

    let components_ctx = override_components.as_ref().or(components);
    let base_ctx = override_base.as_ref().or(base_uri);

    let path_summary = path_item.summary.clone();
    let path_description = path_item.description.clone();
    let path_extensions = path_item.extensions.clone();
    let path_base_path = super::resolve_base_path_from_servers(
        path_item.servers.as_ref(),
        base_ctx.map(|u| u.as_str()),
    )
    .or(base_path.clone());
    let path_servers = map_servers(path_item.servers.as_ref());

    // Handle common parameters defined at PathItem level.
    let common_params_list = path_item.parameters.as_deref().unwrap_or(&[]);
    let common_params = resolve_parameters_with_registry(
        common_params_list,
        components_ctx,
        is_oas3,
        registry,
        base_ctx,
    )?;

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
                components_ctx,
                is_oas3,
                path_base_path.clone(),
                path_servers.clone(),
                path_summary.clone(),
                path_description.clone(),
                path_extensions.clone(),
                global_security,
                operation_ids,
                registry,
                base_ctx,
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
        for (method, op) in additional {
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
            add_op(&method.to_ascii_uppercase(), Some(op.clone()))?;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_route(
    path: &str,
    method: &str,
    op: ShimOperation,
    common_params: &[RouteParam],
    kind: RouteKind,
    components: Option<&ShimComponents>,
    is_oas3: bool,
    path_base_path: Option<String>,
    path_servers: Option<Vec<ParsedServer>>,
    path_summary: Option<String>,
    path_description: Option<String>,
    path_extensions: BTreeMap<String, JsonValue>,
    global_security: Option<&Vec<serde_json::Value>>,
    operation_ids: &mut std::collections::HashSet<String>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<ParsedRoute> {
    let base_path =
        super::resolve_base_path_from_servers(op.servers.as_ref(), base_uri.map(|u| u.as_str()))
            .or(path_base_path);
    let operation_servers = map_servers(op.servers.as_ref());
    // 1. Handler Name
    let handler_name = if let Some(op_id) = &op.operation_id {
        to_snake_case(op_id)
    } else {
        derive_handler_name(method, path)
    };

    // 2. Parameters
    let op_params_list = op.parameters.as_deref().unwrap_or(&[]);
    let op_params =
        resolve_parameters_with_registry(op_params_list, components, is_oas3, registry, base_uri)?;

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
    let mut raw_request_body = None;
    if let Some(req_body_ref) = &op.request_body {
        if let Some(def) =
            extract_request_body_type_with_registry(req_body_ref, components, registry, base_uri)?
        {
            request_body = Some(def);
        }
        if is_oas3 {
            raw_request_body = extract_request_body_raw_with_registry(
                req_body_ref,
                components,
                registry,
                base_uri,
            );
        }
    }

    // Extract Swagger 2.0 "in: formData" params
    let form_params: Vec<_> = params
        .iter()
        .filter(|p| p.source == ParamSource::FormData)
        .cloned()
        .collect();
    if !form_params.is_empty() && request_body.is_none() {
        let is_multipart = form_params.iter().any(|p| p.ty.contains("Vec<u8>"));
        request_body = Some(crate::openapi::parse::models::RequestBodyDefinition {
            ty: if is_multipart {
                "reqwest::multipart::Form".to_string()
            } else {
                "serde_json::Value".to_string()
            },
            description: Some("Form Data".to_string()),
            media_type: if is_multipart {
                "multipart/form-data".to_string()
            } else {
                "application/x-www-form-urlencoded".to_string()
            },
            format: if is_multipart {
                crate::openapi::parse::models::BodyFormat::Multipart
            } else {
                crate::openapi::parse::models::BodyFormat::Form
            },
            required: form_params.iter().any(|p| !p.ty.starts_with("Option<")),
            example: None,
            encoding: None,
            item_encoding: None,
            prefix_encoding: None,
        });
        params.retain(|p| p.source != ParamSource::FormData);
    }

    // 4. Security
    // Operation-level security overrides global security. An explicit empty array clears security.
    let security_defined = op.security.is_some();
    let security = if let Some(requirements) = &op.security {
        if requirements.is_empty() {
            Vec::new()
        } else {
            parse_security_requirements(requirements, components, registry, base_uri)
        }
    } else if let Some(global) = global_security {
        parse_security_requirements(global, components, registry, base_uri)
    } else {
        Vec::new()
    };

    // 5. Response Type, Headers, and Links
    let (
        response_type,
        response_status,
        response_summary,
        response_description,
        response_media_type,
        response_example,
        response_headers,
        response_links,
    ) = if let Some(details) =
        extract_response_details_with_registry(&op.responses, components, registry, base_uri)?
    {
        (
            details.body_type,
            details.status_code,
            details.summary,
            details.description,
            details.media_type,
            details.example,
            details.headers,
            details.links,
        )
    } else {
        (None, None, None, None, None, None, Vec::new(), Vec::new())
    };
    let raw_responses = if is_oas3 {
        Some(op.responses.raw.clone())
    } else {
        None
    };

    // 6. Callbacks
    let mut parsed_callbacks = Vec::new();
    if let Some(cb_map) = &op.callbacks {
        for (cb_name, cb_ref) in cb_map {
            // Resolve RefOr -> BTreeMap<Expression, PathItem>
            let (callback_defs, cb_components, cb_base) =
                resolve_callback_object(cb_ref, components, registry, base_uri)?;
            let callback_components = cb_components.as_ref().or(components);
            let callback_base = cb_base.as_ref().or(base_uri);

            for (expression, path_item) in callback_defs {
                // Flatten the PathItem inside the callback into ParsedCallback entries
                extract_callback_operations(
                    &mut parsed_callbacks,
                    cb_name,
                    &expression,
                    &path_item,
                    callback_components,
                    operation_ids,
                    registry,
                    callback_base,
                    global_security,
                )?;
            }
        }
    }

    // 7. Metadata (Deprecated & External Docs)
    let external_docs = op.external_docs.map(|d| ParsedExternalDocs {
        url: d.url,
        description: d.description,
    });

    let operation_summary = op.summary.clone();
    let operation_description = op.description.clone();
    let summary = operation_summary.clone().or(path_summary.clone());
    let description = operation_description.clone().or(path_description.clone());

    // 8. Tags (Used for module grouping)
    let tags = op.tags.unwrap_or_default();
    let extensions = filter_extensions(&op.extensions);

    Ok(ParsedRoute {
        path: path.to_string(),
        summary,
        description,
        path_summary,
        path_description,
        operation_summary,
        operation_description,
        path_extensions,
        base_path,
        path_servers,
        servers_override: operation_servers,
        method: method.to_string(),
        handler_name,
        operation_id: op.operation_id.clone(),
        params,
        path_params: common_params.to_vec(),
        request_body,
        raw_request_body,
        security,
        security_defined,
        response_type,
        response_status,
        response_summary,
        response_description,
        response_media_type,
        response_example,
        response_headers,
        raw_responses,
        response_links: Some(response_links),
        kind,
        tags,
        callbacks: parsed_callbacks,
        deprecated: op.deprecated,
        external_docs,
        extensions,
    })
}

fn map_servers(servers: Option<&Vec<ShimServer>>) -> Option<Vec<ParsedServer>> {
    let servers = servers?;
    if servers.is_empty() {
        return None;
    }
    Some(servers.iter().map(map_server).collect())
}

fn map_server(server: &ShimServer) -> ParsedServer {
    ParsedServer {
        url: server.url.clone(),
        description: server.description.clone(),
        name: server.name.clone(),
        variables: server
            .variables
            .as_ref()
            .map(|vars| {
                vars.iter()
                    .map(|(k, v)| (k.clone(), map_server_variable(v)))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn map_server_variable(var: &ShimServerVariable) -> ParsedServerVariable {
    ParsedServerVariable {
        enum_values: var.enum_values.clone(),
        default: var.default.clone(),
        description: var.description.clone(),
    }
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

fn filter_extensions(extensions: &BTreeMap<String, JsonValue>) -> BTreeMap<String, JsonValue> {
    extensions
        .iter()
        .filter(|(key, _)| key.starts_with("x-"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

#[cfg(test)]
mod additional_tests {
    use super::*;
    use crate::openapi::parse::routes::shims::{ShimOperation, ShimPathItem};
    use std::collections::BTreeMap;
    use utoipa::openapi::Responses;

    fn empty_operation() -> ShimOperation {
        ShimOperation {
            operation_id: None,
            summary: None,
            description: None,
            parameters: None,
            request_body: None,
            responses: crate::openapi::parse::routes::shims::ShimResponses::from(Responses::new()),
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
            None,
            None,
        )
        .expect_err("expected error");

        assert!(format!("{err}").contains("must be a valid HTTP token"));
    }
}

/// Parses a list of Security Requirement Objects into internal requirements.
///
/// Preserves each Security Requirement object as an alternative (OR semantics).
/// Empty requirement objects (`{}`) represent anonymous access.
pub(crate) fn parse_security_requirements(
    requirements: &[serde_json::Value],
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Vec<crate::openapi::parse::models::SecurityRequirementGroup> {
    let mut security = Vec::new();
    for req in requirements {
        if let Ok(map) = serde_json::from_value::<HashMap<String, Vec<String>>>(req.clone()) {
            if map.is_empty() {
                security.push(crate::openapi::parse::models::SecurityRequirementGroup::anonymous());
                continue;
            }

            let mut schemes = Vec::new();
            for (scheme_name, scopes) in map {
                let scheme = resolve_security_scheme(&scheme_name, components, registry, base_uri);
                schemes.push(SecurityRequirement {
                    scheme_name,
                    scopes,
                    scheme,
                });
            }
            security.push(crate::openapi::parse::models::SecurityRequirementGroup { schemes });
        }
    }
    security
}

fn resolve_security_scheme(
    name: &str,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<SecuritySchemeInfo> {
    if let Some(comps) = components {
        if let Some(schemes) = comps.security_schemes.as_ref() {
            if let Some(ref_or) = schemes.get(name) {
                return resolve_security_scheme_ref_or(ref_or, comps, registry, base_uri);
            }

            // URI-form security scheme reference (OAS 3.2).
            let self_uri = comps.self_uri.as_deref();
            if let Some(ref_name) = extract_component_name(name, self_uri, "securitySchemes") {
                if let Some(ref_or) = schemes.get(&ref_name) {
                    return resolve_security_scheme_ref_or(ref_or, comps, registry, base_uri);
                }
            }
        }
    }

    if let Some(registry) = registry {
        if let Some(raw) = registry.resolve_component_ref(name, base_uri, "securitySchemes") {
            return serde_json::from_value::<ShimSecurityScheme>(raw)
                .ok()
                .map(|shim| build_security_scheme_info(&shim));
        }
    }

    None
}

fn resolve_security_scheme_ref_or(
    ref_or: &RefOr<ShimSecurityScheme>,
    components: &ShimComponents,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<SecuritySchemeInfo> {
    match ref_or {
        RefOr::T(shim) => Some(build_security_scheme_info(shim)),
        RefOr::Ref(r) => {
            resolve_security_scheme(&r.ref_location, Some(components), registry, base_uri)
        }
    }
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
        ShimSecurityScheme::OAuth2(o) => SecuritySchemeKind::OAuth2 {
            flows: Box::new(map_oauth_flows(o)),
            oauth2_metadata_url: o.oauth2_metadata_url.clone(),
        },
        ShimSecurityScheme::OpenIdConnect(o) => SecuritySchemeKind::OpenIdConnect {
            open_id_connect_url: o.open_id_connect_url.clone(),
        },
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

    let deprecated = match shim {
        ShimSecurityScheme::ApiKey(k) => k.deprecated,
        ShimSecurityScheme::Http(h) => h.deprecated,
        ShimSecurityScheme::OAuth2(o) => o.deprecated,
        ShimSecurityScheme::OpenIdConnect(o) => o.deprecated,
        ShimSecurityScheme::MutualTls(m) => m.deprecated,
        ShimSecurityScheme::Basic => false,
    };

    SecuritySchemeInfo {
        kind,
        description,
        deprecated,
    }
}

fn map_oauth_flow(flow: &ShimOAuthFlow) -> OAuthFlow {
    OAuthFlow {
        authorization_url: flow.authorization_url.clone(),
        device_authorization_url: flow.device_authorization_url.clone(),
        token_url: flow.token_url.clone(),
        refresh_url: flow.refresh_url.clone(),
        scopes: flow.scopes.clone(),
    }
}

fn map_oauth_flows(shim: &ShimOAuth2) -> OAuthFlows {
    if let Some(flows) = shim.flows.as_ref() {
        return OAuthFlows {
            implicit: flows.implicit.as_ref().map(map_oauth_flow),
            password: flows.password.as_ref().map(map_oauth_flow),
            client_credentials: flows.client_credentials.as_ref().map(map_oauth_flow),
            authorization_code: flows.authorization_code.as_ref().map(map_oauth_flow),
            device_authorization: flows.device_authorization.as_ref().map(map_oauth_flow),
        };
    }

    map_legacy_oauth_flows(shim)
}

fn map_legacy_oauth_flows(shim: &ShimOAuth2) -> OAuthFlows {
    let mut flows = OAuthFlows {
        implicit: None,
        password: None,
        client_credentials: None,
        authorization_code: None,
        device_authorization: None,
    };

    let scopes = shim.scopes.clone().unwrap_or_default();
    match shim.flow.as_deref() {
        Some("implicit") => {
            flows.implicit = Some(OAuthFlow {
                authorization_url: shim.authorization_url.clone(),
                device_authorization_url: None,
                token_url: None,
                refresh_url: None,
                scopes,
            });
        }
        Some("password") => {
            flows.password = Some(OAuthFlow {
                authorization_url: None,
                device_authorization_url: None,
                token_url: shim.token_url.clone(),
                refresh_url: None,
                scopes,
            });
        }
        Some("application") => {
            flows.client_credentials = Some(OAuthFlow {
                authorization_url: None,
                device_authorization_url: None,
                token_url: shim.token_url.clone(),
                refresh_url: None,
                scopes,
            });
        }
        Some("accessCode") => {
            flows.authorization_code = Some(OAuthFlow {
                authorization_url: shim.authorization_url.clone(),
                device_authorization_url: None,
                token_url: shim.token_url.clone(),
                refresh_url: None,
                scopes,
            });
        }
        _ => {}
    }

    flows
}

#[cfg(test)]
mod tests {
    use crate::openapi::parse::models::RuntimeExpression;
    use crate::openapi::parse::models::SecuritySchemeKind;

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
        let routes =
            super::super::parse_openapi_routes(yaml).expect("Failed to parse openapi routes");
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
        let routes =
            super::super::parse_openapi_routes(yaml).expect("Failed to parse openapi routes");
        let scheme = routes[0]
            .security
            .first()
            .and_then(|group| group.schemes.first())
            .and_then(|s| s.scheme.clone())
            .expect("expected resolved scheme");

        assert!(matches!(scheme.kind, SecuritySchemeKind::ApiKey { .. }));
    }

    #[test]
    fn test_security_scheme_oauth2_and_oidc_details() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Security, version: 1.0}
components:
  securitySchemes:
    oauth:
      type: oauth2
      oauth2MetadataUrl: https://auth.example.com/.well-known/oauth-authorization-server
      flows:
        authorizationCode:
          authorizationUrl: https://auth.example.com/authorize
          tokenUrl: https://auth.example.com/token
          scopes:
            read: Read data
    oidc:
      type: openIdConnect
      openIdConnectUrl: https://auth.example.com/.well-known/openid-configuration
paths:
  /secure:
    get:
      security:
        - oauth: [read]
        - oidc: []
      responses:
        '200': {description: ok}
"#;
        let routes =
            super::super::parse_openapi_routes(yaml).expect("Failed to parse openapi routes");
        let oauth = routes[0]
            .security
            .iter()
            .flat_map(|group| group.schemes.iter())
            .find(|s| s.scheme_name == "oauth")
            .and_then(|s| s.scheme.clone())
            .expect("expected oauth scheme");
        let oidc = routes[0]
            .security
            .iter()
            .flat_map(|group| group.schemes.iter())
            .find(|s| s.scheme_name == "oidc")
            .and_then(|s| s.scheme.clone())
            .expect("expected oidc scheme");

        let oauth_dbg = format!("{:?}", oauth.kind);
        assert!(
            oauth_dbg.contains("https://auth.example.com/.well-known/oauth-authorization-server")
        );
        assert!(oauth_dbg.contains("https://auth.example.com/authorize"));
        assert!(oauth_dbg.contains("https://auth.example.com/token"));
        assert!(oauth_dbg.contains("Read data"));

        assert_eq!(
            oauth.kind,
            SecuritySchemeKind::OAuth2 {
                flows: Box::new(crate::openapi::parse::models::OAuthFlows {
                    implicit: None,
                    password: None,
                    client_credentials: None,
                    authorization_code: Some(crate::openapi::parse::models::OAuthFlow {
                        authorization_url: Some("https://auth.example.com/authorize".to_string()),
                        token_url: Some("https://auth.example.com/token".to_string()),
                        refresh_url: None,
                        device_authorization_url: None,
                        scopes: std::iter::once(("read".to_string(), "Read data".to_string()))
                            .collect(),
                    }),
                    device_authorization: None,
                }),
                oauth2_metadata_url: Some(
                    "https://auth.example.com/.well-known/oauth-authorization-server".to_string()
                ),
            }
        );

        assert_eq!(
            oidc.kind,
            SecuritySchemeKind::OpenIdConnect {
                open_id_connect_url: "https://auth.example.com/.well-known/openid-configuration"
                    .to_string(),
            }
        );

        // Cover Swagger 2.0 body & formData (multipart & form), legacy OAuth2 flows (implicit, password, application, accessCode, unknown),
        // Basic auth, MutualTls, ApiKey (query, cookie, unknown), Ref to local & external securityScheme, empty servers,
        // non-object security requirement, empty responses, externalDocs, and all tchar characters
        let mut reg = crate::openapi::parse::registry::DocumentRegistry::new();
        reg.register_openapi_yaml(
            "https://example.com/sec.yaml",
            r#"
openapi: 3.2.0
info: { title: Sec, version: "1.0" }
paths: {}
components:
  securitySchemes:
    ExtMtls:
      type: mutualTLS
      description: mTLS
      deprecated: true
"#,
        )
        .expect("reg");

        let rich_yaml = r#"
openapi: 3.2.0
$self: https://example.com/api.yaml
info: { title: Rich, version: "1.0" }
components:
  securitySchemes:
    QueryKey:
      type: apiKey
      name: qk
      in: query
    CookieKey:
      type: apiKey
      name: ck
      in: cookie
    HttpAuth:
      type: http
      scheme: bearer
    RefToLocal:
      $ref: '#/components/securitySchemes/QueryKey'
    RefToExt:
      $ref: 'https://example.com/sec.yaml#/components/securitySchemes/ExtMtls'
paths:
  /all-methods:
    servers: []
    parameters:
      - name: shared
        in: query
        schema: { type: string }
    put:
      parameters:
        - name: shared
          in: query
          schema: { type: string }
      externalDocs:
        url: https://example.com/docs
        description: Docs
      x-custom: 1
      security:
        - {}
        - QueryKey: []
          CookieKey: []
          HttpAuth: []
          RefToLocal: []
          RefToExt: []
          'https://example.com/api.yaml#/components/securitySchemes/CookieKey': []
          'https://example.com/api.yaml#/components/securitySchemes/MissingUri': []
      responses:
        '200': { description: ok }
    delete:
      responses: { '200': { description: ok } }
    patch:
      responses: { '200': { description: ok } }
    options:
      responses: { '200': { description: ok } }
    head:
      responses: { '200': { description: ok } }
    trace:
      responses: { '200': { description: ok } }
    query:
      responses: { '200': { description: ok } }
    additionalOperations:
      "M!#$%&'*+-.^_`|~1":
        responses: { '200': { description: ok } }
"#;
        let rich_routes = super::super::parse_openapi_routes_with_registry(
            rich_yaml,
            Some(&reg),
            Some("https://example.com/api.yaml"),
        )
        .expect("rich routes");
        assert_eq!(rich_routes.len(), 8);

        // Cover parse_security_requirements with non-map JSON value, unknown scheme, and fallback ApiKey / Legacy OAuth2 flow
        let mut fallback_comps: crate::openapi::parse::routes::shims::ShimComponents =
            serde_yaml::from_str(
                r#"
securitySchemes:
  OtherKey:
    type: apiKey
    name: ok
    in: other
  RefMissing:
    $ref: '#/components/securitySchemes/Missing'
  RefMissingExt:
    $ref: 'https://example.com/sec.yaml#/components/securitySchemes/MissingExt'
  RefValidExt:
    $ref: 'https://example.com/sec.yaml#/components/securitySchemes/ExtMtls'
  BasicScheme:
    type: basic
  OAuthImplicit:
    type: oauth2
    flow: implicit
    authorizationUrl: https://example.com/auth
  OAuthPassword:
    type: oauth2
    flow: password
    tokenUrl: https://example.com/token
  OAuthApp:
    type: oauth2
    flow: application
    tokenUrl: https://example.com/token
  OAuthCode:
    type: oauth2
    flow: accessCode
    authorizationUrl: https://example.com/auth
    tokenUrl: https://example.com/token
  OAuthUnknown:
    type: oauth2
    flow: custom
"#,
            )
            .expect("fallback_comps");
        fallback_comps.self_uri = Some("https://example.com/api.yaml".to_string());
        let _non_map_reqs = super::parse_security_requirements(
            &[
                serde_json::json!("not_a_map"),
                serde_json::json!({
                    "OtherKey": [],
                    "RefMissing": [],
                    "RefMissingExt": [],
                    "RefValidExt": [],
                    "BasicScheme": [],
                    "OAuthImplicit": [],
                    "OAuthPassword": [],
                    "OAuthApp": [],
                    "OAuthCode": [],
                    "OAuthUnknown": [],
                    "UnknownDirect": [],
                    "https://example.com/sec.yaml#/components/securitySchemes/ExtMtls": []
                }),
            ],
            Some(&fallback_comps),
            Some(&reg),
            None,
        );
        let _ = super::parse_security_requirements(
            &[serde_json::json!({
                "https://example.com/sec.yaml#/components/securitySchemes/MissingExt": []
            })],
            Some(&crate::openapi::parse::routes::shims::ShimComponents::default()),
            Some(&reg),
            None,
        );

        // Cover Swagger 2.0 body, formData (form and multipart), Basic auth, and legacy OAuth2 flows
        let sw2_yaml = r#"
swagger: "2.0"
info: { title: Sw2, version: "1.0" }
securityDefinitions:
  basicAuth:
    type: basic
  oauthImplicit:
    type: oauth2
    flow: implicit
    authorizationUrl: https://example.com/auth
  oauthPassword:
    type: oauth2
    flow: password
    tokenUrl: https://example.com/token
  oauthApp:
    type: oauth2
    flow: application
    tokenUrl: https://example.com/token
  oauthCode:
    type: oauth2
    flow: accessCode
    authorizationUrl: https://example.com/auth
    tokenUrl: https://example.com/token
paths:
  /form-url:
    post:
      security:
        - basicAuth: []
          oauthImplicit: []
          oauthPassword: []
          oauthApp: []
          oauthCode: []
      parameters:
        - name: field1
          in: formData
          type: string
          required: true
      responses:
        '200': { description: ok }
  /form-mp:
    post:
      parameters:
        - name: file1
          in: formData
          type: file
      responses:
        '200': { description: ok }
"#;
        let sw2_routes = super::super::parse_openapi_routes(sw2_yaml).expect("sw2 routes");
        assert_eq!(sw2_routes.len(), 2);

        // Cover parse_path_item error paths and map_server_variable directly
        let mut p_err = crate::openapi::parse::routes::shims::ShimPathItem {
            ref_path: None,
            summary: None,
            description: None,
            servers: Some(vec![crate::openapi::parse::routes::shims::ShimServer {
                url: "https://{env}.example.com".to_string(),
                description: None,
                name: None,
                variables: Some({
                    let mut m = std::collections::BTreeMap::new();
                    m.insert(
                        "env".to_string(),
                        crate::openapi::parse::routes::shims::ShimServerVariable {
                            enum_values: None,
                            default: "prod".to_string(),
                            description: None,
                            extensions: std::collections::BTreeMap::new(),
                        },
                    );
                    m
                }),
                extensions: std::collections::BTreeMap::new(),
            }]),
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
            additional_operations: Some({
                let mut m = std::collections::BTreeMap::new();
                m.insert(
                    "get".to_string(),
                    crate::openapi::parse::routes::shims::ShimOperation {
                        operation_id: None,
                        summary: None,
                        description: None,
                        parameters: None,
                        request_body: None,
                        responses: crate::openapi::parse::routes::shims::ShimResponses::from(
                            utoipa::openapi::Responses::new(),
                        ),
                        security: None,
                        callbacks: None,
                        tags: None,
                        deprecated: false,
                        external_docs: None,
                        servers: None,
                        extensions: std::collections::BTreeMap::new(),
                    },
                );
                m
            }),
            extensions: std::collections::BTreeMap::new(),
        };
        let mut r_out = Vec::new();
        assert!(super::parse_path_item(
            &mut r_out,
            "/res",
            &p_err,
            crate::openapi::parse::models::RouteKind::Path,
            None,
            true,
            None,
            None,
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashSet::new(),
            &std::collections::BTreeMap::new(),
            None,
            None,
        )
        .is_err());

        // Cover build_route querystring conflicts via common_params + op_params
        p_err.additional_operations = None;
        let qs_shim: crate::openapi::parse::resolver::ShimParameter = serde_json::from_value(
            serde_json::json!({
                "name": "qs1",
                "in": "querystring",
                "content": { "application/x-www-form-urlencoded": { "schema": { "type": "object" } } }
            }),
        )
        .expect("qs1");
        let qs2_shim: crate::openapi::parse::resolver::ShimParameter = serde_json::from_value(
            serde_json::json!({
                "name": "qs2",
                "in": "querystring",
                "content": { "application/x-www-form-urlencoded": { "schema": { "type": "object" } } }
            }),
        )
        .expect("qs2");
        let q_shim: crate::openapi::parse::resolver::ShimParameter =
            serde_json::from_value(serde_json::json!({
                "name": "q1",
                "in": "query",
                "schema": { "type": "string" }
            }))
            .expect("q1");

        p_err.parameters = Some(vec![utoipa::openapi::RefOr::T(qs_shim)]);
        p_err.get = Some(crate::openapi::parse::routes::shims::ShimOperation {
            operation_id: None,
            summary: None,
            description: None,
            parameters: Some(vec![utoipa::openapi::RefOr::T(qs2_shim)]),
            request_body: None,
            responses: crate::openapi::parse::routes::shims::ShimResponses::from(
                utoipa::openapi::Responses::new(),
            ),
            security: None,
            callbacks: None,
            tags: None,
            deprecated: false,
            external_docs: None,
            servers: None,
            extensions: std::collections::BTreeMap::new(),
        });
        assert!(super::parse_path_item(
            &mut r_out,
            "/qs-dup",
            &p_err,
            crate::openapi::parse::models::RouteKind::Path,
            None,
            true,
            None,
            None,
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashSet::new(),
            &std::collections::BTreeMap::new(),
            None,
            None,
        )
        .is_err());

        p_err.get.as_mut().expect("get").parameters = Some(vec![utoipa::openapi::RefOr::T(q_shim)]);
        assert!(super::parse_path_item(
            &mut r_out,
            "/qs-mix",
            &p_err,
            crate::openapi::parse::models::RouteKind::Path,
            None,
            true,
            None,
            None,
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashSet::new(),
            &std::collections::BTreeMap::new(),
            None,
            None,
        )
        .is_err());

        p_err.get.as_mut().expect("get").parameters = None;
        super::parse_path_item(
            &mut r_out,
            "/qs-ok",
            &p_err,
            crate::openapi::parse::models::RouteKind::Path,
            None,
            true,
            None,
            None,
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashSet::new(),
            &std::collections::BTreeMap::new(),
            None,
            None,
        )
        .expect("qs ok");

        // Cover formData (form and multipart), duplicate operationId, and validate_path_parameters errors directly in parse_path_item
        let fd_file: crate::openapi::parse::resolver::ShimParameter =
            serde_json::from_value(serde_json::json!({
                "name": "f2",
                "in": "formData",
                "type": "file"
            }))
            .expect("fd_file");
        let fd_str: crate::openapi::parse::resolver::ShimParameter =
            serde_json::from_value(serde_json::json!({
                "name": "f1",
                "in": "formData",
                "type": "string",
                "required": true
            }))
            .expect("fd_str");
        let path_param_id: crate::openapi::parse::resolver::ShimParameter =
            serde_json::from_value(serde_json::json!({
                "name": "id",
                "in": "path",
                "required": true,
                "schema": { "type": "string" }
            }))
            .expect("path_param_id");

        p_err.parameters = Some(vec![utoipa::openapi::RefOr::T(fd_str.clone())]);
        p_err.get.as_mut().expect("get").operation_id = Some("dupId".to_string());
        let mut op_ids = std::collections::HashSet::new();
        super::parse_path_item(
            &mut r_out,
            "/fd-form",
            &p_err,
            crate::openapi::parse::models::RouteKind::Path,
            None,
            false,
            None,
            None,
            &mut std::collections::HashMap::new(),
            &mut op_ids,
            &std::collections::BTreeMap::new(),
            None,
            None,
        )
        .expect("fd form");
        assert!(super::parse_path_item(
            &mut r_out,
            "/fd-form-dup",
            &p_err,
            crate::openapi::parse::models::RouteKind::Path,
            None,
            false,
            None,
            None,
            &mut std::collections::HashMap::new(),
            &mut op_ids,
            &std::collections::BTreeMap::new(),
            None,
            None,
        )
        .is_err());

        p_err.get.as_mut().expect("get").operation_id = None;
        p_err.get.as_mut().expect("get").request_body = None;
        p_err.parameters = Some(vec![utoipa::openapi::RefOr::T(fd_str.clone())]);
        super::parse_path_item(
            &mut r_out,
            "/fd-str-no-rb",
            &p_err,
            crate::openapi::parse::models::RouteKind::Path,
            None,
            false,
            None,
            None,
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashSet::new(),
            &std::collections::BTreeMap::new(),
            None,
            None,
        )
        .expect("fd str no rb");

        p_err.parameters = Some(vec![utoipa::openapi::RefOr::T(fd_file.clone())]);
        super::parse_path_item(
            &mut r_out,
            "/fd-mp-no-rb",
            &p_err,
            crate::openapi::parse::models::RouteKind::Path,
            None,
            false,
            None,
            None,
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashSet::new(),
            &std::collections::BTreeMap::new(),
            None,
            None,
        )
        .expect("fd mp no rb");

        p_err.get.as_mut().expect("get").request_body = Some(utoipa::openapi::RefOr::T(
            serde_json::from_value(serde_json::json!({
                "required": true,
                "content": { "application/json": { "schema": { "type": "string" } } }
            }))
            .expect("rb true"),
        ));
        p_err.parameters = Some(vec![utoipa::openapi::RefOr::T(fd_file.clone())]);
        super::parse_path_item(
            &mut r_out,
            "/fd-mp-with-rb",
            &p_err,
            crate::openapi::parse::models::RouteKind::Path,
            None,
            false,
            None,
            None,
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashSet::new(),
            &std::collections::BTreeMap::new(),
            None,
            None,
        )
        .expect("fd mp with rb");

        let mut p_empty_add = p_err.clone();
        let mut add_map = std::collections::BTreeMap::new();
        add_map.insert("".to_string(), p_err.get.clone().expect("get"));
        p_empty_add.additional_operations = Some(add_map);
        assert!(super::parse_path_item(
            &mut r_out,
            "/empty-add",
            &p_empty_add,
            crate::openapi::parse::models::RouteKind::Path,
            None,
            false,
            None,
            None,
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashSet::new(),
            &std::collections::BTreeMap::new(),
            None,
            None,
        )
        .is_err());

        p_err.get.as_mut().expect("get").request_body = Some(utoipa::openapi::RefOr::T(
            serde_json::from_value(serde_json::json!({
                "required": false,
                "content": { "application/json": { "schema": false } }
            }))
            .expect("rb false"),
        ));
        p_err.parameters = Some(vec![utoipa::openapi::RefOr::T(fd_file)]);
        super::parse_path_item(
            &mut r_out,
            "/fd-mp",
            &p_err,
            crate::openapi::parse::models::RouteKind::Path,
            None,
            false,
            None,
            None,
            &mut std::collections::HashMap::new(),
            &mut std::collections::HashSet::new(),
            &std::collections::BTreeMap::new(),
            None,
            None,
        )
        .expect("fd mp");

        p_err.parameters = Some(vec![utoipa::openapi::RefOr::T(path_param_id)]);
        for bad_path in ["/items/{id}/{id}", "/items/{id}/{other}", "/items"] {
            assert!(super::parse_path_item(
                &mut r_out,
                bad_path,
                &p_err,
                crate::openapi::parse::models::RouteKind::Path,
                None,
                true,
                None,
                None,
                &mut std::collections::HashMap::new(),
                &mut std::collections::HashSet::new(),
                &std::collections::BTreeMap::new(),
                None,
                None,
            )
            .is_err());
        }
    }
}
