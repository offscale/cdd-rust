#![deny(missing_docs)]

//! Parsing logic for OpenAPI Paths and Webhooks.
//!
//! Iterates over the `paths` and `webhooks` sections of the YAML and converts operations
//! (GET, POST, etc.) into `ParsedRoute` definitions with resolved parameters
//! and security requirements.

use crate::error::{AppError, AppResult};
use crate::oas::models::{ParsedRoute, RouteKind, RouteParam, SecurityRequirement};
use crate::oas::resolver::{
    extract_request_body_type, extract_response_success_type, resolve_parameters, ShimParameter,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use utoipa::openapi::request_body::RequestBody;
use utoipa::openapi::{RefOr, Responses};

/// Shim structs to strictly enforce RefOr parsing and Generic Components.
///
/// We use these instead of utoipa root structs directly to ensure we capture raw structures correctly
/// and can use our `ShimParameter` for robust deserialization.
#[derive(Deserialize)]
struct ShimOpenApi {
    #[serde(default)]
    components: Option<Value>,
    #[serde(default)]
    paths: BTreeMap<String, ShimPathItem>,
    #[serde(default)]
    webhooks: Option<BTreeMap<String, RefOr<ShimPathItem>>>,
}

#[derive(Deserialize)]
struct ShimPathItem {
    #[serde(default)]
    parameters: Option<Vec<RefOr<ShimParameter>>>,
    get: Option<ShimOperation>,
    post: Option<ShimOperation>,
    put: Option<ShimOperation>,
    delete: Option<ShimOperation>,
    patch: Option<ShimOperation>,
    options: Option<ShimOperation>,
    head: Option<ShimOperation>,
    trace: Option<ShimOperation>,
}

#[derive(Deserialize)]
struct ShimOperation {
    #[serde(rename = "operationId")]
    operation_id: Option<String>,
    #[serde(default)]
    parameters: Option<Vec<RefOr<ShimParameter>>>,
    #[serde(rename = "requestBody")]
    request_body: Option<RefOr<RequestBody>>,
    #[serde(default)]
    responses: Responses,
    #[serde(default)]
    security: Option<Vec<Value>>, // Parsing raw security to generic maps
}

/// Parses a raw OpenAPI YAML string and extracts route definitions from `paths` and `webhooks`.
pub fn parse_openapi_routes(yaml_content: &str) -> AppResult<Vec<ParsedRoute>> {
    let openapi: ShimOpenApi = serde_yaml::from_str(yaml_content)
        .map_err(|e| AppError::General(format!("Failed to parse OpenAPI YAML: {}", e)))?;

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

fn parse_path_item(
    routes: &mut Vec<ParsedRoute>,
    path_or_name: &str,
    path_item: ShimPathItem,
    kind: RouteKind,
    components: Option<&Value>,
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

    Ok(())
}

fn build_route(
    path: &str,
    method: &str,
    op: ShimOperation,
    common_params: &[RouteParam],
    kind: RouteKind,
    components: Option<&Value>,
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
    if let Some(req_body_ref) = op.request_body {
        if let Some(def) = extract_request_body_type(&req_body_ref)? {
            request_body = Some(def);
        }
    }

    // 4. Security
    let mut security = Vec::new();
    if let Some(requirements) = op.security {
        for req in requirements {
            if let Ok(map) = serde_json::from_value::<HashMap<String, Vec<String>>>(req) {
                for (scheme, scopes) in map {
                    security.push(SecurityRequirement {
                        scheme_name: scheme,
                        scopes,
                    });
                }
            }
        }
    }

    // 5. Response Type
    let response_type = extract_response_success_type(&op.responses, components)?;

    Ok(ParsedRoute {
        path: path.to_string(),
        method: method.to_string(),
        handler_name,
        params,
        request_body,
        security,
        response_type,
        kind,
    })
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            for lc in c.to_lowercase() {
                result.push(lc);
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn derive_handler_name(method: &str, path: &str) -> String {
    let clean_path = path.replace(['{', '}'], "").replace('/', "_");
    format!(
        "{}_{}",
        method.to_lowercase(),
        clean_path.trim_start_matches('_')
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::ParamSource;

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
}
