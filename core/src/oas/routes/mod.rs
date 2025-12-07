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
use crate::oas::routes::shims::ShimOpenApi;
use utoipa::openapi::RefOr;

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

    let mut routes = Vec::new();
    let components = openapi.components.as_ref();

    // 1. Parse standard Paths
    for (path_str, path_item) in openapi.paths {
        parse_path_item(
            &mut routes,
            &path_str,
            path_item,
            RouteKind::Path,
            components,
        )?;
    }

    // 2. Parse Webhooks
    if let Some(webhooks) = openapi.webhooks {
        for (name, path_item_or_ref) in webhooks {
            // We resolve the RefOr by assuming it is inline for now,
            // resolving webhook Refs requires root-level resolution logic not in this scope.
            if let RefOr::T(path_item) = path_item_or_ref {
                parse_path_item(
                    &mut routes,
                    &name,
                    path_item,
                    RouteKind::Webhook,
                    components,
                )?;
            }
        }
    }

    Ok(routes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::ParamSource;
    use crate::oas::routes::shims::ShimOpenApi;

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
        assert_eq!(cb.expression, "{$request.body#/url}");
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
        assert_eq!(cb.expression, "{$request.query.url}");
        assert_eq!(cb.method, "PUT");
    }
}
