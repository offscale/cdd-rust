#![deny(missing_docs)]

//! # Route Builder
//!
//! Logic that transforms `Shim` structs into `ParsedRoute` IR models.

use crate::error::{AppError, AppResult};
use crate::oas::models::{
    OAuthFlow, OAuthFlows, ParamSource, ParsedRoute, ParsedServer, ParsedServerVariable, RouteKind,
    RouteParam, SecurityRequirement, SecuritySchemeInfo, SecuritySchemeKind,
};
use crate::oas::ref_utils::extract_component_name;
use crate::oas::registry::DocumentRegistry;
use crate::oas::resolver::body::{
    extract_request_body_raw_with_registry, extract_request_body_type_with_registry,
};
use crate::oas::resolver::resolve_parameters_with_registry;
use crate::oas::resolver::responses::extract_response_details_with_registry;
use crate::oas::routes::callbacks::{extract_callback_operations, resolve_callback_object};
use crate::oas::routes::naming::{derive_handler_name, to_snake_case};
use crate::oas::routes::shims::{
    ShimComponents, ShimOAuth2, ShimOAuthFlow, ShimOperation, ShimPathItem, ShimSecurityScheme,
    ShimServer, ShimServerVariable,
};
use crate::parser::ParsedExternalDocs;
use regex::Regex;
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, HashMap};
use url::Url;
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
            None,
            None,
        )
        .unwrap_err();

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
) -> Vec<crate::oas::models::SecurityRequirementGroup> {
    let mut security = Vec::new();
    for req in requirements {
        if let Ok(map) = serde_json::from_value::<HashMap<String, Vec<String>>>(req.clone()) {
            if map.is_empty() {
                security.push(crate::oas::models::SecurityRequirementGroup::anonymous());
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
            security.push(crate::oas::models::SecurityRequirementGroup { schemes });
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
                if let Some(info) =
                    resolve_security_scheme_ref_or(ref_or, schemes, comps, registry, base_uri)
                {
                    return Some(info);
                }
            }

            // URI-form security scheme reference (OAS 3.2).
            let self_uri = comps.extra.get("__self").and_then(|v| v.as_str());
            if let Some(ref_name) = extract_component_name(name, self_uri, "securitySchemes") {
                if let Some(ref_or) = schemes.get(&ref_name) {
                    if let Some(info) =
                        resolve_security_scheme_ref_or(ref_or, schemes, comps, registry, base_uri)
                    {
                        return Some(info);
                    }
                }
            }
        }
    }

    if let Some(registry) = registry {
        if let Some(raw) = registry.resolve_component_ref(name, base_uri, "securitySchemes") {
            if let Ok(shim) = serde_json::from_value::<ShimSecurityScheme>(raw) {
                return Some(build_security_scheme_info(&shim));
            }
        }
    }

    None
}

fn resolve_security_scheme_ref_or(
    ref_or: &RefOr<ShimSecurityScheme>,
    schemes: &BTreeMap<String, RefOr<ShimSecurityScheme>>,
    components: &ShimComponents,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<SecuritySchemeInfo> {
    match ref_or {
        RefOr::T(shim) => Some(build_security_scheme_info(shim)),
        RefOr::Ref(r) => {
            resolve_security_scheme_ref(&r.ref_location, schemes, components, registry, base_uri)
        }
    }
}

fn resolve_security_scheme_ref(
    ref_str: &str,
    schemes: &BTreeMap<String, RefOr<ShimSecurityScheme>>,
    components: &ShimComponents,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<SecuritySchemeInfo> {
    let self_uri = components.extra.get("__self").and_then(|v| v.as_str());
    if let Some(ref_name) = extract_component_name(ref_str, self_uri, "securitySchemes") {
        if let Some(RefOr::T(shim)) = schemes.get(&ref_name) {
            return Some(build_security_scheme_info(shim));
        }
    }

    if let Some(registry) = registry {
        if let Some(raw) = registry.resolve_component_ref(ref_str, base_uri, "securitySchemes") {
            if let Ok(shim) = serde_json::from_value::<ShimSecurityScheme>(raw) {
                return Some(build_security_scheme_info(&shim));
            }
        }
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
        ShimSecurityScheme::OAuth2(o) => SecuritySchemeKind::OAuth2 {
            flows: map_oauth_flows(o),
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
        let routes = super::super::parse_openapi_routes(yaml).unwrap();
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

        match oauth.kind {
            SecuritySchemeKind::OAuth2 {
                flows,
                oauth2_metadata_url,
            } => {
                assert_eq!(
                    oauth2_metadata_url.as_deref(),
                    Some("https://auth.example.com/.well-known/oauth-authorization-server")
                );
                assert_eq!(
                    flows
                        .authorization_code
                        .as_ref()
                        .and_then(|f| f.authorization_url.as_deref()),
                    Some("https://auth.example.com/authorize")
                );
                assert_eq!(
                    flows
                        .authorization_code
                        .as_ref()
                        .and_then(|f| f.token_url.as_deref()),
                    Some("https://auth.example.com/token")
                );
                assert_eq!(
                    flows
                        .authorization_code
                        .as_ref()
                        .and_then(|f| f.scopes.get("read")),
                    Some(&"Read data".to_string())
                );
            }
            _ => panic!("expected OAuth2 security scheme"),
        }

        match oidc.kind {
            SecuritySchemeKind::OpenIdConnect {
                open_id_connect_url,
            } => {
                assert_eq!(
                    open_id_connect_url,
                    "https://auth.example.com/.well-known/openid-configuration"
                );
            }
            _ => panic!("expected OpenID Connect security scheme"),
        }
    }
}
