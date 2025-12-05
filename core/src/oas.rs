#![deny(missing_docs)]

//! # OpenAPI Parsing
//!
//! Parses OpenAPI 3.1 specifications (YAML) into internal representations.
//! Supports extracting `ParsedStruct` (schemas) and `ParsedRoute` (paths).
//! Uses `utoipa` definitions for mapping.

use crate::error::{AppError, AppResult};
use crate::parser::{ParsedField, ParsedStruct};
use utoipa::openapi::path::{Operation, Parameter, ParameterIn};
use utoipa::openapi::request_body::RequestBody;
use utoipa::openapi::schema::{ArrayItems, KnownFormat, Schema, SchemaFormat, SchemaType, Type};
use utoipa::openapi::{OpenApi, RefOr};

/// Represents a parsed API route.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedRoute {
    /// The URL path, e.g. "/users/{id}"
    pub path: String,
    /// HTTP Method: "GET", "POST", etc.
    pub method: String,
    /// Rust handler name, typically snake_case of operationId
    pub handler_name: String,
    /// Route parameters (path, query)
    pub params: Vec<RouteParam>,
    /// Request body type name (if any). e.g. "CreateUserRequest"
    pub request_body: Option<String>,
}

/// Represents a parameter in a route (Path or Query).
#[derive(Debug, Clone, PartialEq)]
pub struct RouteParam {
    /// Parameter name in the source (e.g. "id")
    pub name: String,
    /// Whether it's from Path or Query
    pub source: ParamSource,
    /// Rust type (e.g. "Uuid", "i32", "Option<String>")
    pub ty: String,
}

/// The source location of a parameter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParamSource {
    /// URL Path parameter (e.g. /users/{id})
    Path,
    /// URL Query parameter (e.g. /users?page=1)
    Query,
}

/// Parses a raw OpenAPI YAML string and extracts route definitions from `paths`.
///
/// Maps:
/// - `{path_var}` (in: path) -> Actix Path compatible types.
/// - `requestBody` -> Actix Json compatible types. (Returns strict type name, wrapper is implied).
///
/// # Arguments
///
/// * `yaml_content` - The raw YAML string of the openapi.yaml.
///
/// # Returns
///
/// * `Vec<ParsedRoute>` - A list of routes extracted from the paths.
pub fn parse_openapi_routes(yaml_content: &str) -> AppResult<Vec<ParsedRoute>> {
    let openapi: OpenApi = serde_yaml::from_str(yaml_content)
        .map_err(|e| AppError::General(format!("Failed to parse OpenAPI YAML: {}", e)))?;

    let mut routes = Vec::new();

    // Iterate over paths.
    for (path_str, path_item) in openapi.paths.paths {
        // Handle common parameters defined at PathItem level.
        let common_params_list = path_item.parameters.as_deref().unwrap_or(&[]);
        let common_params = resolve_parameters(common_params_list)?;

        // operations
        if let Some(op) = path_item.get {
            routes.push(build_route(&path_str, "GET", op, &common_params)?);
        }
        if let Some(op) = path_item.post {
            routes.push(build_route(&path_str, "POST", op, &common_params)?);
        }
        if let Some(op) = path_item.put {
            routes.push(build_route(&path_str, "PUT", op, &common_params)?);
        }
        if let Some(op) = path_item.delete {
            routes.push(build_route(&path_str, "DELETE", op, &common_params)?);
        }
        if let Some(op) = path_item.patch {
            routes.push(build_route(&path_str, "PATCH", op, &common_params)?);
        }
    }

    Ok(routes)
}

/// Parses a raw OpenAPI YAML string and extracts struct definitions from `components/schemas`.
///
/// # Arguments
///
/// * `yaml_content` - The raw YAML string of the openapi.yaml.
///
/// # Returns
///
/// * `Vec<ParsedStruct>` - A list of structs extracted from the schema definitions.
pub fn parse_openapi_spec(yaml_content: &str) -> AppResult<Vec<ParsedStruct>> {
    let openapi: OpenApi = serde_yaml::from_str(yaml_content)
        .map_err(|e| AppError::General(format!("Failed to parse OpenAPI YAML: {}", e)))?;

    let components = openapi
        .components
        .ok_or_else(|| AppError::General("No components found in OpenAPI spec".into()))?;

    let mut parsed_structs = Vec::new();

    for (name, ref_or_schema) in components.schemas {
        if let RefOr::T(Schema::Object(obj)) = ref_or_schema {
            if obj.schema_type == SchemaType::Type(Type::Object) {
                let required_set = &obj.required;
                let mut fields = Vec::new();

                for (field_name, field_schema) in obj.properties {
                    let is_required = required_set.contains(&field_name);
                    let rust_type = map_schema_to_rust_type(&field_schema, is_required)?;

                    let description = match &field_schema {
                        RefOr::T(Schema::Object(o)) => o.description.clone(),
                        _ => None,
                    };

                    fields.push(ParsedField {
                        name: field_name,
                        ty: rust_type,
                        description,
                        rename: None,
                        is_skipped: false,
                    });
                }

                parsed_structs.push(ParsedStruct {
                    name,
                    description: obj.description,
                    rename: None,
                    fields,
                });
            }
        }
    }

    Ok(parsed_structs)
}

// --- Helper Functions ---

fn build_route(
    path: &str,
    method: &str,
    op: Operation,
    common_params: &[RouteParam],
) -> AppResult<ParsedRoute> {
    // 1. Handler Name
    // Prefer operationId, otherwise derive from method + path
    let handler_name = if let Some(op_id) = &op.operation_id {
        to_snake_case(op_id)
    } else {
        derive_handler_name(method, path)
    };

    // 2. Parameters
    // Merge common params with operation params.
    let op_params_list = op.parameters.as_deref().unwrap_or(&[]);
    let op_params = resolve_parameters(op_params_list)?;

    let mut params = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Priority to operation params
    for p in op_params {
        seen.insert((p.name.clone(), p.source.clone()));
        params.push(p);
    }
    // Add common params if not overridden
    for p in common_params {
        if !seen.contains(&(p.name.clone(), p.source.clone())) {
            params.push(p.clone());
        }
    }

    // 3. Request Body
    let mut request_body = None;
    if let Some(req_body_ref) = op.request_body {
        if let Some(name) = extract_request_body_type(&RefOr::from(req_body_ref))? {
            request_body = Some(name);
        }
    }

    Ok(ParsedRoute {
        path: path.to_string(),
        method: method.to_string(),
        handler_name,
        params,
        request_body,
    })
}

fn resolve_parameters(params: &[Parameter]) -> AppResult<Vec<RouteParam>> {
    let mut result = Vec::new();
    for param in params {
        // param is strictly Parameter struct
        let name = param.name.clone();
        let source = match param.parameter_in {
            ParameterIn::Path => ParamSource::Path,
            ParameterIn::Query => ParamSource::Query,
            _ => continue, // Ignore header/cookie
        };

        // utoipa 5: Check Required enum.
        let is_required = param.required == utoipa::openapi::Required::True;

        let ty = if let Some(schema_ref) = &param.schema {
            map_schema_to_rust_type(schema_ref, is_required)?
        } else {
            "String".to_string() // Default
        };

        result.push(RouteParam { name, source, ty });
    }
    Ok(result)
}

fn extract_request_body_type(body: &RefOr<RequestBody>) -> AppResult<Option<String>> {
    let content = match body {
        RefOr::T(b) => &b.content,
        RefOr::Ref(_) => return Ok(None),
    };

    if let Some(media) = content.get("application/json") {
        if let Some(schema_ref) = &media.schema {
            // Treat as required to extract inner type (T) instead of Option<T>
            let type_str = map_schema_to_rust_type(schema_ref, true)?;
            return Ok(Some(type_str));
        }
    }
    Ok(None)
}

fn map_schema_to_rust_type(schema: &RefOr<Schema>, is_required: bool) -> AppResult<String> {
    let type_str = match schema {
        RefOr::Ref(r) => {
            let path = &r.ref_location;
            path.split('/').next_back().unwrap_or("Unknown").to_string()
        }
        RefOr::T(s) => match s {
            Schema::Object(obj) => match obj.schema_type {
                SchemaType::Type(Type::Integer) => match &obj.format {
                    Some(SchemaFormat::KnownFormat(KnownFormat::Int64)) => "i64".to_string(),
                    _ => "i32".to_string(),
                },
                SchemaType::Type(Type::Number) => match &obj.format {
                    Some(SchemaFormat::KnownFormat(KnownFormat::Float)) => "f32".to_string(),
                    _ => "f64".to_string(),
                },
                SchemaType::Type(Type::Boolean) => "bool".to_string(),
                SchemaType::Type(Type::String) => match &obj.format {
                    Some(SchemaFormat::KnownFormat(KnownFormat::Uuid)) => "Uuid".to_string(),
                    Some(SchemaFormat::KnownFormat(KnownFormat::DateTime)) => {
                        "DateTime".to_string()
                    }
                    Some(SchemaFormat::KnownFormat(KnownFormat::Date)) => "NaiveDate".to_string(),
                    _ => "String".to_string(),
                },
                SchemaType::Type(Type::Array) => "Vec<serde_json::Value>".to_string(),
                _ => "serde_json::Value".to_string(),
            },
            Schema::Array(arr) => match &arr.items {
                ArrayItems::RefOrSchema(boxed_schema) => {
                    let inner_type = map_schema_to_rust_type(boxed_schema, true)?;
                    format!("Vec<{}>", inner_type)
                }
                _ => "Vec<serde_json::Value>".to_string(),
            },
            _ => "serde_json::Value".to_string(),
        },
    };

    if is_required {
        Ok(type_str)
    } else {
        Ok(format!("Option<{}>", type_str))
    }
}

// Very basic camelCase -> snake_case conversion
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
    // GET /users/{id} -> get_users_id
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

    #[test]
    fn test_parse_routes_basic() {
        let yaml = r#"
openapi: 3.1.0
info:
  title: T
  version: 1.0
paths:
  /users/{id}:
    get:
      operationId: getUserById
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
            format: uuid
      responses:
        '200':
          description: OK
    post:
      operationId: UpdateUser
      requestBody:
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/UpdateUserRequest'
      responses:
        '200':
          description: OK
"#;
        let routes = parse_openapi_routes(yaml).unwrap();

        // GET
        let get_r = routes.iter().find(|r| r.method == "GET").unwrap();
        assert_eq!(get_r.path, "/users/{id}");
        assert_eq!(get_r.handler_name, "get_user_by_id");
        assert_eq!(get_r.params.len(), 1);
        assert_eq!(get_r.params[0].name, "id");
        assert_eq!(get_r.params[0].source, ParamSource::Path);
        assert_eq!(get_r.params[0].ty, "Uuid");
        assert!(get_r.request_body.is_none());

        // POST
        let post_r = routes.iter().find(|r| r.method == "POST").unwrap();
        assert_eq!(post_r.handler_name, "update_user");
        assert_eq!(post_r.request_body.as_deref(), Some("UpdateUserRequest"));
        assert!(post_r.params.is_empty());
    }

    #[test]
    fn test_parse_routes_common_params() {
        let yaml = r#"
openapi: 3.1.0
info: {title: T, version: 1}
paths:
  /items:
    parameters:
      - name: tenant_id
        in: query
        required: false
        schema: {type: integer}
    get:
      operationId: listItems
      responses:
        '200':
          description: OK
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let r = &routes[0];
        assert_eq!(r.method, "GET");
        assert_eq!(r.params.len(), 1);
        assert_eq!(r.params[0].name, "tenant_id");
        assert_eq!(r.params[0].ty, "Option<i32>"); // Not required -> Option
    }

    #[test]
    fn test_derive_handler_name() {
        let yaml = r#"
openapi: 3.1.0
info: {title: T, version: 1}
paths:
  /users/{id}/details:
    get:
      responses:
        '200':
          description: OK
"#;
        // No operationId
        let routes = parse_openapi_routes(yaml).unwrap();
        let r = &routes[0];
        // GET /users/{id}/details -> get_users_id_details
        assert_eq!(r.handler_name, "get_users_id_details");
    }
}
