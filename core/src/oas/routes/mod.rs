#![deny(missing_docs)]

//! # Routes Module
//!
//! Entry point for parsing OpenAPI `paths` and `webhooks`.
//! Orchestrates the Parsing of Shims -> Builder -> IR Models.

pub mod builder;
pub mod callbacks;
pub mod naming;
pub mod shims;

use crate::error::{AppError, AppResult};
use crate::oas::models::{ParsedRoute, RouteKind};
use crate::oas::routes::builder::parse_path_item;
use crate::oas::routes::shims::{ShimComponents, ShimOpenApi, ShimPathItem, ShimServer};
use utoipa::openapi::RefOr;
use std::collections::{BTreeMap, HashSet};

/// Parses a raw OpenAPI YAML string and extracts route definitions from `paths` and `webhooks`.
///
/// This function verifies the presence of a valid `openapi` (3.x) or `swagger` (2.0) version field
/// before processing the routes.
pub fn parse_openapi_routes(yaml_content: &str) -> AppResult<Vec<ParsedRoute>> {
    let openapi: ShimOpenApi = serde_yaml::from_str(yaml_content)
        .map_err(|e| AppError::General(format!("Failed to parse OpenAPI YAML: {}", e)))?;

    // Version Validation
    if let Some(version) = &openapi.openapi {
        if !(version as &String).starts_with("3.") {
            return Err(AppError::General(format!(
                "Unsupported OpenAPI version: {}. Only 3.x is supported by this parser.",
                version
            )));
        }
    } else if let Some(version) = &openapi.swagger {
        if !version.starts_with("2.") {
            return Err(AppError::General(format!(
                "Unsupported Swagger version: {}. Only 2.0 is supported for legacy compatibility.",
                version
            )));
        }
    } else {
        return Err(AppError::General(
            "Invalid OpenAPI document: missing 'openapi' or 'swagger' version field.".into(),
        ));
    }

    let base_path = resolve_base_path(&openapi);
    let mut routes = Vec::new();
    let components = openapi.components.as_ref();
    let global_security = openapi.security.as_ref();

    // 1. Parse standard Paths
    for (path_str, path_item) in &openapi.paths {
        parse_path_item(
            &mut routes,
            path_str,
            path_item,
            RouteKind::Path,
            components,
            base_path.clone(),
            global_security,
            &openapi.paths,
        )?;
    }

    // 2. Parse Webhooks
    if let Some(webhooks) = &openapi.webhooks {
        for (name, path_item_or_ref) in webhooks {
            let resolved = match path_item_or_ref {
                RefOr::T(path_item) => path_item.clone(),
                RefOr::Ref(r) => resolve_path_item_ref(&r.ref_location, components, &openapi.paths)?,
            };

            parse_path_item(
                &mut routes,
                name,
                &resolved,
                RouteKind::Webhook,
                components,
                None, // Webhooks generally don't use server prefix logic like paths
                global_security,
                &openapi.paths,
            )?;
        }
    }

    Ok(routes)
}

/// Resolves the base URL path from `servers` (OAS 3) or `basePath` (Swagger 2).
///
/// For servers:
/// - Takes the first server.
/// - Resolves variables with `default` values.
/// - Extracts the path component.
fn resolve_base_path(openapi: &ShimOpenApi) -> Option<String> {
    // 1. Swagger 2.0 basePath
    if let Some(bp) = &openapi.base_path {
        let trimmed = bp.trim_end_matches('/');
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    // 2. OpenAPI 3.x servers
    resolve_base_path_from_servers(openapi.servers.as_ref())
}

/// Resolves a base path from an OAS `servers` array.
fn resolve_base_path_from_servers(servers: Option<&Vec<ShimServer>>) -> Option<String> {
    let servers = servers?;
    let server = servers.first()?;
    let mut url = server.url.clone();

    // OAS 3.2: Resolve server variables with their default values
    if let Some(vars) = &server.variables {
        for (key, var_shim) in vars {
            let placeholder = format!("{{{}}}", key);
            url = url.replace(&placeholder, &var_shim.default);
        }
    }

    // Strip host if present, keep path (Basic implementation)
    // "https://api.com/v1" -> "/v1"
    if let Some(idx) = url.find("://") {
        let after_scheme = &url[idx + 3..];
        if let Some(slash_idx) = after_scheme.find('/') {
            let path = &after_scheme[slash_idx..];
            let trimmed = path.trim_end_matches('/');
            return if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        return None; // Root path
    } else if url.starts_with('/') {
        let trimmed = url.trim_end_matches('/');
        return if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }

    None
}

/// Resolves a `$ref` pointing to a Path Item from components or local paths.
fn resolve_path_item_ref(
    ref_str: &str,
    components: Option<&ShimComponents>,
    paths: &BTreeMap<String, ShimPathItem>,
) -> AppResult<ShimPathItem> {
    let mut visited = HashSet::new();
    resolve_path_item_ref_inner(ref_str, components, paths, &mut visited)
}

fn resolve_path_item_ref_inner(
    ref_str: &str,
    components: Option<&ShimComponents>,
    paths: &BTreeMap<String, ShimPathItem>,
    visited: &mut HashSet<String>,
) -> AppResult<ShimPathItem> {
    if !visited.insert(ref_str.to_string()) {
        return Err(AppError::General(format!(
            "PathItem reference cycle detected at {}",
            ref_str
        )));
    }

    if let Some(pointer) = ref_str.strip_prefix("#/") {
        let segments: Vec<&str> = pointer.split('/').collect();
        if segments.get(0) == Some(&"components") && segments.get(1) == Some(&"pathItems") {
            let name_seg = segments.get(2).ok_or_else(|| {
                AppError::General("PathItem reference missing name".into())
            })?;
            if segments.len() > 3 {
                return Err(AppError::General(format!(
                    "Unsupported PathItem reference depth: {}",
                    ref_str
                )));
            }
            let name = decode_pointer_segment(name_seg);
            if let Some(comps) = components {
                if let Some(map) = &comps.path_items {
                    if let Some(ref_or) = map.get(&name) {
                        match ref_or {
                            RefOr::T(pi) => return Ok(pi.clone()),
                            RefOr::Ref(r) => {
                                return resolve_path_item_ref_inner(
                                    &r.ref_location,
                                    components,
                                    paths,
                                    visited,
                                )
                            }
                        }
                    }
                }
                if let Some(path_items_val) = comps.extra.get("pathItems") {
                    if let Some(item_val) = path_items_val.get(&name) {
                        let parsed =
                            serde_json::from_value::<ShimPathItem>(
                                item_val.clone(),
                            )
                            .map_err(|e| {
                                AppError::General(format!(
                                    "Failed to parse PathItem '{}': {}",
                                    name, e
                                ))
                            })?;
                        if let Some(next_ref) = parsed.ref_path.as_deref() {
                            return resolve_path_item_ref_inner(
                                next_ref, components, paths, visited,
                            );
                        }
                        return Ok(parsed);
                    }
                }
            }
            return Err(AppError::General(format!(
                "PathItem reference not found: {}",
                ref_str
            )));
        }

        if segments.get(0) == Some(&"paths") {
            let name_seg = segments
                .get(1)
                .ok_or_else(|| AppError::General("Path reference missing name".into()))?;
            if segments.len() > 2 {
                return Err(AppError::General(format!(
                    "Unsupported Path reference depth: {}",
                    ref_str
                )));
            }
            let path_key = decode_pointer_segment(name_seg);
            if let Some(pi) = paths.get(&path_key) {
                if let Some(next_ref) = pi.ref_path.as_deref() {
                    return resolve_path_item_ref_inner(next_ref, components, paths, visited);
                }
                return Ok(pi.clone());
            }
            return Err(AppError::General(format!(
                "Path reference not found: {}",
                ref_str
            )));
        }
    }

    Err(AppError::General(format!(
        "Unsupported PathItem reference: {}",
        ref_str
    )))
}

fn decode_pointer_segment(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::{ParamSource, RuntimeExpression};
    use crate::oas::routes::shims::ShimOpenApi;
    use std::collections::BTreeMap;

    #[test]
    fn test_parse_routes_basic() {
        let yaml = r#"
openapi: 3.1.0
info: {title: T, version: 1.0}
paths:
  /users/{id}:
    get:
      operationId: getUserById
      parameters:
        - name: id
          in: path
          required: true
          schema: {type: string, format: uuid}
      responses: { '200': {description: OK} }
    post:
      operationId: UpdateUser
      requestBody:
        content:
          application/json:
            schema: { $ref: '#/components/schemas/UpdateUserRequest' }
      responses:
        '200': {description: OK, content: {application/json: {schema: {type: object}}}}
"#;
        let routes = parse_openapi_routes(yaml).unwrap();

        let get_r = routes.iter().find(|r| r.method == "GET").unwrap();
        assert_eq!(get_r.params[0].name, "id");
        assert_eq!(get_r.params[0].source, ParamSource::Path);

        let post_r = routes.iter().find(|r| r.method == "POST").unwrap();
        let body = post_r.request_body.as_ref().unwrap();
        assert_eq!(body.ty, "UpdateUserRequest");
    }

    #[test]
    fn test_parse_oas_3_2_0_compliant() {
        let yaml = r#"
openapi: 3.2.0
jsonSchemaDialect: https://spec.openapis.org/oas/3.1/dialect/base
info:
  title: OAS 3.2 Test
  version: 1.0.0
servers:
  - url: https://api.example.com/v1
    description: Production Server
paths:
  /ping:
    get:
      responses: { '200': {description: Pong} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/ping");
        assert_eq!(routes[0].base_path.as_deref(), Some("/v1"));
    }

    #[test]
    fn test_parse_legacy_swagger_2_0() {
        let yaml = r#"
swagger: "2.0"
info: {title: Legacy, version: 1.0}
paths:
  /legacy:
    get:
      responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/legacy");
    }

    #[test]
    fn test_server_variable_resolution() {
        let yaml = r#"
openapi: 3.2.0
info: {title: S, version: 1}
servers:
  - url: https://{env}.api.com/v1
    variables:
      env:
        default: staging
        enum: [staging, production]
paths:
  /users: # Should resolve to /v1/users in test-gen
    get:
      responses: {}
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes[0].base_path.as_deref(), Some("/v1"));
    }

    #[test]
    fn test_swagger_2_base_path() {
        let yaml = r#"
swagger: "2.0"
info: {title: Legacy, version: 1.0}
basePath: /api/legacy
paths:
  /old:
    get:
      responses: {}
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes[0].base_path.as_deref(), Some("/api/legacy"));
    }

    #[test]
    fn test_missing_version_fails() {
        let yaml = r#"
info: {title: Missing Version, version: 1.0}
paths: {}
"#;
        let res = parse_openapi_routes(yaml);
        assert!(res.is_err());
        match res.unwrap_err() {
            AppError::General(msg) => assert!(msg.contains("missing 'openapi' or 'swagger'")),
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_metadata_deserialization() {
        // Direct test of ShimOpenApi to verify Metadata Objects Parsing requirement
        // independent of what `parse_openapi_routes` returns.
        let yaml = r#"
openapi: 3.2.0
info:
  title: Detailed API
  description: Markdown *supported*
  termsOfService: https://example.com/terms
  contact:
    name: Support
    email: support@example.com
  license:
    name: MIT
    identifier: MIT
  version: 1.2.3
servers:
  - url: https://{env}.example.com
    variables:
      env:
        default: dev
        enum: [dev, prod]
externalDocs:
  url: https://docs.example.com
  description: Context
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(openapi.openapi.as_deref(), Some("3.2.0"));

        let info = openapi.info.unwrap();
        assert_eq!(
            info.terms_of_service.as_deref(),
            Some("https://example.com/terms")
        );

        let contact = info.contact.unwrap();
        assert_eq!(contact.email.as_deref(), Some("support@example.com"));

        let license = info.license.unwrap();
        assert_eq!(license.identifier.as_deref(), Some("MIT"));

        let servers = openapi.servers.unwrap();
        assert_eq!(servers[0].url, "https://{env}.example.com");
        let vars = servers[0].variables.as_ref().unwrap();
        assert_eq!(vars.get("env").unwrap().default, "dev");

        let ext = openapi.external_docs.unwrap();
        assert_eq!(ext.url, "https://docs.example.com");
    }

    #[test]
    fn test_route_parsing_with_reusable_params() {
        let yaml = r#"
openapi: 3.1.0
info: {title: Reuse, version: 1.0}
components:
  parameters:
    limitParam:
      name: limit
      in: query
      schema: {type: integer}
    userId:
      name: id
      in: path
      required: true
      schema: {type: string, format: uuid}
paths:
  /users/{id}:
    parameters:
      - $ref: '#/components/parameters/userId'
    get:
      parameters:
        - $ref: '#/components/parameters/limitParam'
      responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let r = &routes[0];

        assert_eq!(r.params.len(), 2);

        // userId should be parsed (common param)
        let id_p = r
            .params
            .iter()
            .find(|p| p.name == "id")
            .expect("id param missing");
        assert_eq!(id_p.source, ParamSource::Path);

        // limitParam should be parsed (Op param)
        let limit_p = r
            .params
            .iter()
            .find(|p| p.name == "limit")
            .expect("limit param missing");
        assert_eq!(limit_p.source, ParamSource::Query);
        assert!(limit_p.ty.contains("i32"));
    }

    #[test]
    fn test_global_security_applies() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Sec, version: 1.0}
components:
  securitySchemes:
    api_key:
      type: apiKey
      name: api_key
      in: header
security:
  - api_key: []
paths:
  /secure:
    get:
      responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].security.len(), 1);
        assert_eq!(routes[0].security[0].scheme_name, "api_key");
    }

    #[test]
    fn test_operation_security_overrides_global() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Sec, version: 1.0}
components:
  securitySchemes:
    api_key:
      type: apiKey
      name: api_key
      in: header
    oauth:
      type: oauth2
      flows:
        implicit:
          authorizationUrl: https://auth.example.com
          scopes: { read: read }
security:
  - api_key: []
paths:
  /secure:
    get:
      security:
        - oauth: [read]
      responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].security.len(), 1);
        assert_eq!(routes[0].security[0].scheme_name, "oauth");
        assert_eq!(routes[0].security[0].scopes, vec!["read"]);
    }

    #[test]
    fn test_operation_security_empty_clears_global() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Sec, version: 1.0}
components:
  securitySchemes:
    api_key:
      type: apiKey
      name: api_key
      in: header
security:
  - api_key: []
paths:
  /public:
    get:
      security: []
      responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert!(routes[0].security.is_empty());
    }

    #[test]
    fn test_querystring_conflict_with_query() {
        let yaml = r#"
openapi: 3.2.0
info: {title: QueryString, version: 1.0}
paths:
  /search:
    get:
      parameters:
        - name: raw
          in: querystring
          content:
            application/x-www-form-urlencoded:
              schema: { type: object }
        - name: q
          in: query
          schema: { type: string }
      responses: { '200': {description: OK} }
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("mixes 'querystring' and 'query'"));
    }

    #[test]
    fn test_querystring_duplicate_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: QueryString, version: 1.0}
paths:
  /search:
    parameters:
      - name: base
        in: querystring
        content:
          application/x-www-form-urlencoded:
            schema: { type: object }
    get:
      parameters:
        - name: override
          in: querystring
          content:
            application/x-www-form-urlencoded:
              schema: { type: object }
      responses: { '200': {description: OK} }
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("multiple querystring parameters"));
    }

    #[test]
    fn test_parse_routes_with_reusable_response() {
        let yaml = r#"
openapi: 3.1.0
info: {title: Res, version: 1}
components:
  schemas:
    MyData: { type: object }
  responses:
    SuccessResponse:
      description: S
      content:
        application/json:
          schema: { $ref: '#/components/schemas/MyData' }
paths:
  /data:
    get:
      responses:
        '200': { $ref: '#/components/responses/SuccessResponse' }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let r = &routes[0];
        assert_eq!(r.response_type.as_deref(), Some("MyData"));
    }

    #[test]
    fn test_path_item_ref_resolution() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Ref, version: 1.0}
components:
  pathItems:
    UserPath:
      get:
        responses: { '200': {description: OK} }
paths:
  /users:
    $ref: '#/components/pathItems/UserPath'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/users");
        assert_eq!(routes[0].method, "GET");
    }

    #[test]
    fn test_additional_operations_parsing() {
        let yaml = r#"
openapi: 3.2.0
info: {title: ExtraOps, version: 1.0}
paths:
  /copy:
    additionalOperations:
      GET:
        operationId: ignoredGet
        responses: { '200': {description: OK} }
      COPY:
        operationId: copyThing
        responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        let r = &routes[0];
        assert_eq!(r.method, "COPY");
        assert_eq!(r.handler_name, "copy_thing");
    }

    #[test]
    fn test_servers_override_precedence() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Servers, version: 1.0}
servers:
  - url: https://api.example.com/v1
paths:
  /items:
    servers:
      - url: https://api.example.com/v2
    get:
      servers:
        - url: https://api.example.com/v3
      responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].base_path.as_deref(), Some("/v3"));
    }

    #[test]
    fn test_webhook_ref_resolution() {
        let yaml = r#"
openapi: 3.2.0
info: {title: WebhookRef, version: 1.0}
components:
  pathItems:
    HookItem:
      post:
        responses: { '200': {description: OK} }
webhooks:
  userCreated:
    $ref: '#/components/pathItems/HookItem'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "userCreated");
        assert_eq!(routes[0].method, "POST");
        assert_eq!(routes[0].kind, crate::oas::models::RouteKind::Webhook);
    }

    #[test]
    fn test_path_ref_to_paths_section() {
        let yaml = r#"
openapi: 3.2.0
info: {title: PathRef, version: 1.0}
paths:
  /base:
    get:
      responses: { '200': {description: OK} }
  /alias:
    $ref: '#/paths/~1base'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 2);
        let alias_route = routes.iter().find(|r| r.path == "/alias").unwrap();
        assert_eq!(alias_route.method, "GET");
    }

    #[test]
    fn test_path_item_ref_from_components_extra() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "pathItems".to_string(),
            serde_json::json!({
                "ExtraItem": {
                    "get": { "responses": { "200": { "description": "OK" } } }
                }
            }),
        );

        let paths = BTreeMap::new();
        let resolved = resolve_path_item_ref(
            "#/components/pathItems/ExtraItem",
            Some(&components),
            &paths,
        )
        .unwrap();
        assert!(resolved.get.is_some());
    }

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
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        let route = &routes[0];

        assert_eq!(route.callbacks.len(), 1);
        let cb = &route.callbacks[0];
        assert_eq!(cb.name, "onData");
        assert_eq!(
            cb.expression,
            RuntimeExpression::new("{$request.body#/url}".to_string())
        );
        assert_eq!(cb.method, "POST");
        assert!(cb.request_body.is_some());
    }

    #[test]
    fn test_parse_callback_ref() {
        let yaml = r#"
openapi: 3.1.0
info: {title: Ref Callback, version: 1.0}
components:
  callbacks:
    MyCallback:
      '{$request.query.url}':
        put:
          responses: { '200': {description: OK} }
paths:
  /hook:
    post:
      responses: { '200': {description: OK} }
      callbacks:
        myHook:
          $ref: '#/components/callbacks/MyCallback'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let route = &routes[0];

        assert_eq!(route.callbacks.len(), 1);
        let cb = &route.callbacks[0];
        assert_eq!(cb.name, "myHook");
        assert_eq!(
            cb.expression,
            RuntimeExpression::new("{$request.query.url}")
        );
        assert_eq!(cb.method, "PUT");
    }
}
