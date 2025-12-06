#![deny(missing_docs)]

//! # Resolver Module
//!
//! Logic for resolving OpenAPI Schema definitions into Rust types.
//!
//! Handles:
//! - Recursive type mapping.
//! - Parameter resolution (Inline and Reference) via `ShimParameter` to ensure robust deserialization.
//! - Response reference resolution used in "Reusable Responses".
//! - Body content extraction.

use crate::error::AppResult;
use crate::oas::models::{BodyFormat, ParamSource, RequestBodyDefinition, RouteParam};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use utoipa::openapi::request_body::RequestBody;
use utoipa::openapi::schema::{ArrayItems, KnownFormat, Schema, SchemaFormat, SchemaType, Type};
use utoipa::openapi::{RefOr, Responses};

/// A local shim for Parameter to ensure robust parsing of fields like `required`.
/// Utoipa's Parameter struct can be strict about enums or missing fields.
/// We use this shim to decouple parsing logic from strict library types.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct ShimParameter {
    /// Name of the parameter.
    pub name: String,
    /// Location of the parameter (query, path, header, cookie).
    #[serde(rename = "in")]
    pub parameter_in: String,
    /// Whether the parameter is required.
    #[serde(default)]
    pub required: bool,
    /// Schema definition.
    pub schema: Option<RefOr<Schema>>,
}

// Manual Debug implementation because utoipa::openapi::RefOr<Schema> does not implement Debug.
impl fmt::Debug for ShimParameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShimParameter")
            .field("name", &self.name)
            .field("parameter_in", &self.parameter_in)
            .field("required", &self.required)
            .field("schema", &"Schema(..)") // Opaque formatting for Schema
            .finish()
    }
}

/// Maps an OpenAPI Schema definition to a Rust type string.
pub fn map_schema_to_rust_type(schema: &RefOr<Schema>, is_required: bool) -> AppResult<String> {
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
            Schema::OneOf(_) | Schema::AnyOf(_) | Schema::AllOf(_) => {
                "serde_json::Value".to_string()
            }
            _ => "serde_json::Value".to_string(),
        },
    };

    if is_required {
        Ok(type_str)
    } else {
        Ok(format!("Option<{}>", type_str))
    }
}

/// Resolves a list of OpenAPI parameters into internal `RouteParam` structs.
///
/// Accepts `RefOr<ShimParameter>`, allowing reusable parameters from `components/parameters`.
///
/// # Arguments
///
/// * `params` - The parameter list from the path item or operation.
/// * `components` - The parsed OpenAPI components (optional, used for Ref resolution).
pub fn resolve_parameters(
    params: &[RefOr<ShimParameter>],
    components: Option<&Value>,
) -> AppResult<Vec<RouteParam>> {
    let mut result = Vec::new();
    for param_or_ref in params {
        let param_opt = match param_or_ref {
            RefOr::T(param) => Some(param.clone()),
            RefOr::Ref(r) => resolve_parameter_ref(r, components),
        };

        if let Some(param) = param_opt {
            let route_param = process_parameter(&param)?;
            result.push(route_param);
        }
    }
    Ok(result)
}

/// Helper to convert a resolved ShimParameter to our internal RouteParam.
fn process_parameter(param: &ShimParameter) -> AppResult<RouteParam> {
    let name = param.name.clone();
    let source = match param.parameter_in.as_str() {
        "path" => ParamSource::Path,
        "query" => ParamSource::Query,
        "header" => ParamSource::Header,
        "cookie" => ParamSource::Cookie,
        _ => ParamSource::Query, // Fallback
    };

    let ty = if let Some(schema_ref) = &param.schema {
        map_schema_to_rust_type(schema_ref, param.required)?
    } else {
        "String".to_string() // Default
    };

    Ok(RouteParam { name, source, ty })
}

/// Helper to resolve a `Ref` to its target Parameter definition.
fn resolve_parameter_ref(
    r: &utoipa::openapi::Ref,
    components: Option<&Value>,
) -> Option<ShimParameter> {
    let ref_name = r.ref_location.split('/').next_back()?;

    if let Some(comps) = components {
        // Look in components.parameters.ref_name
        if let Some(param_json) = comps.get("parameters").and_then(|p| p.get(ref_name)) {
            // Attempt to deserialize the JSON into ShimParameter
            if let Ok(param) = serde_json::from_value::<ShimParameter>(param_json.clone()) {
                return Some(param);
            }
        }
    }
    None
}

/// Extracts the request body type and format from the OpenAPI definition.
pub fn extract_request_body_type(
    body: &RefOr<RequestBody>,
) -> AppResult<Option<RequestBodyDefinition>> {
    let content = match body {
        RefOr::T(b) => &b.content,
        RefOr::Ref(_) => return Ok(None),
    };

    // 1. JSON
    if let Some(media) = content.get("application/json") {
        if let Some(schema_ref) = &media.schema {
            let type_str = map_schema_to_rust_type(schema_ref, true)?;
            return Ok(Some(RequestBodyDefinition {
                ty: type_str,
                format: BodyFormat::Json,
            }));
        }
    }

    // 2. Form Url Encoded
    if let Some(media) = content.get("application/x-www-form-urlencoded") {
        if let Some(schema_ref) = &media.schema {
            let type_str = map_schema_to_rust_type(schema_ref, true)?;
            return Ok(Some(RequestBodyDefinition {
                ty: type_str,
                format: BodyFormat::Form,
            }));
        }
    }

    // 3. Multipart
    if let Some(media) = content.get("multipart/form-data") {
        let type_str = if let Some(schema_ref) = &media.schema {
            map_schema_to_rust_type(schema_ref, true)?
        } else {
            // Fallback if no schema is defined
            "Multipart".to_string()
        };

        return Ok(Some(RequestBodyDefinition {
            ty: type_str,
            format: BodyFormat::Multipart,
        }));
    }

    Ok(None)
}

/// Extracts the success response type (200 OK or 201 Created).
///
/// Supports inline definitions and `$ref` pointers to `components/responses`.
///
/// # Arguments
///
/// * `responses` - The Responses object from the operation.
/// * `components` - The parsed components (for resolving Refs).
pub fn extract_response_success_type(
    responses: &Responses,
    components: Option<&Value>,
) -> AppResult<Option<String>> {
    // Check 200 then 201
    let success = responses
        .responses
        .get("200")
        .or_else(|| responses.responses.get("201"));

    if let Some(resp_item) = success {
        let response = match resp_item {
            RefOr::T(val) => Some(val.clone()),
            RefOr::Ref(r) => resolve_response_from_components(r, components),
        };

        if let Some(r) = response {
            // Check JSON content
            if let Some(media) = r.content.get("application/json") {
                if let Some(schema) = &media.schema {
                    let ty = map_schema_to_rust_type(schema, true)?;
                    return Ok(Some(ty));
                }
            }
        }
    }

    Ok(None)
}

/// Helper to resolve a `RefOr::Ref` to the concrete `Response` object via components.
fn resolve_response_from_components(
    r: &utoipa::openapi::Ref,
    components: Option<&Value>,
) -> Option<utoipa::openapi::Response> {
    let ref_name = r.ref_location.split('/').next_back()?;
    if let Some(comps) = components {
        if let Some(resp_json) = comps.get("responses").and_then(|r| r.get(ref_name)) {
            if let Ok(resp) = serde_json::from_value::<utoipa::openapi::Response>(resp_json.clone())
            {
                return Some(resp);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::openapi::{Content, ResponseBuilder};

    #[test]
    fn test_extract_inline_response() {
        let response = ResponseBuilder::new()
            .description("Inline")
            .content(
                "application/json",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/User",
                )))),
            )
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let ty = extract_response_success_type(&responses, None)
            .unwrap()
            .unwrap();
        assert_eq!(ty, "User");
    }

    #[test]
    fn test_extract_response_ref() {
        // Construct raw JSON to simulate components with responses
        let components_json = serde_json::json!({
            "responses": {
                "UserResponse": {
                    "description": "Success",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/User" }
                        }
                    }
                }
            }
        });

        // Test Ref response
        let mut responses = Responses::new();
        responses.responses.insert(
            "200".into(),
            RefOr::Ref(utoipa::openapi::Ref::new(
                "#/components/responses/UserResponse",
            )),
        );

        let ty = extract_response_success_type(&responses, Some(&components_json))
            .unwrap()
            .expect("Should resolve response ref");

        assert_eq!(ty, "User");
    }

    #[test]
    fn test_resolve_reusable_parameters() {
        // 1. Define reusable parameter JSON
        // Note: required is omitted, should default to false
        let components_json = serde_json::json!({
            "parameters": {
                "limitParam": {
                    "name": "limit",
                    "in": "query",
                    "schema": { "type": "integer" }
                }
            }
        });

        // 2. Define list using Ref
        let op_params = vec![RefOr::Ref(utoipa::openapi::Ref::new(
            "#/components/parameters/limitParam",
        ))];

        // 3. Resolve
        let resolved = resolve_parameters(&op_params, Some(&components_json)).unwrap();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "limit");
        assert_eq!(resolved[0].source, ParamSource::Query);
        assert_eq!(resolved[0].ty, "Option<i32>"); // Not required
    }
}
