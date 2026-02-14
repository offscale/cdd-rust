#![deny(missing_docs)]

//! # Type Mapping
//!
//! Logic for mapping OpenAPI Schema definitions to Rust strings.
//!
//! Updates include support for the **OAS Format Registry**:
//! - `format: password` -> `Secret<String>`
//! - `format: float` -> `f32`
//! - `format: double` -> `f64`

use crate::error::AppResult;
use serde_json::Value as JsonValue;
use utoipa::openapi::schema::{ArrayItems, KnownFormat, Schema, SchemaFormat, SchemaType, Type};
use utoipa::openapi::RefOr;

/// Maps an OpenAPI Schema definition to a Rust type string.
///
/// # Arguments
///
/// * `schema` - The schema definition (Ref or Object).
/// * `is_required` - Whether the field is mandatory (wraps in `Option` if false).
pub fn map_schema_to_rust_type(schema: &RefOr<Schema>, is_required: bool) -> AppResult<String> {
    let (type_str, nullable) = match schema {
        RefOr::Ref(r) => {
            let path = &r.ref_location;
            (
                path.split('/').next_back().unwrap_or("Unknown").to_string(),
                false,
            )
        }
        RefOr::T(s) => match s {
            Schema::Object(obj) => {
                if is_binary_schema(obj) {
                    ("Vec<u8>".to_string(), false)
                } else {
                    let (schema_type, nullable) = normalize_schema_type(&obj.schema_type);
                    let type_str = match schema_type {
                        SchemaType::Type(Type::Integer) => match &obj.format {
                            Some(SchemaFormat::KnownFormat(KnownFormat::Int64)) => {
                                "i64".to_string()
                            }
                            _ => "i32".to_string(),
                        },
                        SchemaType::Type(Type::Number) => match &obj.format {
                            Some(SchemaFormat::KnownFormat(KnownFormat::Float)) => {
                                "f32".to_string()
                            }
                            Some(SchemaFormat::KnownFormat(KnownFormat::Double)) => {
                                "f64".to_string()
                            }
                            // Default for number without format is f64 in Rust
                            _ => "f64".to_string(),
                        },
                        SchemaType::Type(Type::Boolean) => "bool".to_string(),
                        SchemaType::Type(Type::String) => match &obj.format {
                            Some(SchemaFormat::KnownFormat(KnownFormat::Uuid)) => {
                                "Uuid".to_string()
                            }
                            Some(SchemaFormat::KnownFormat(KnownFormat::DateTime)) => {
                                "DateTime".to_string()
                            }
                            Some(SchemaFormat::KnownFormat(KnownFormat::Date)) => {
                                "NaiveDate".to_string()
                            }
                            Some(SchemaFormat::KnownFormat(KnownFormat::Password)) => {
                                "Secret<String>".to_string()
                            }
                            Some(SchemaFormat::KnownFormat(KnownFormat::Byte))
                            | Some(SchemaFormat::KnownFormat(KnownFormat::Binary)) => {
                                "Vec<u8>".to_string()
                            }
                            Some(SchemaFormat::Custom(custom))
                                if matches!(
                                    custom.to_ascii_lowercase().as_str(),
                                    "byte" | "binary"
                                ) =>
                            {
                                "Vec<u8>".to_string()
                            }
                            _ => "String".to_string(),
                        },
                        SchemaType::Type(Type::Array) => "Vec<serde_json::Value>".to_string(),
                        SchemaType::Type(Type::Object)
                        | SchemaType::AnyValue
                        | SchemaType::Array(_) => "serde_json::Value".to_string(),
                        SchemaType::Type(Type::Null) => "serde_json::Value".to_string(),
                    };
                    (type_str, nullable)
                }
            }
            Schema::Array(arr) => match &arr.items {
                ArrayItems::RefOrSchema(boxed_schema) => {
                    let inner_type = map_schema_to_rust_type(boxed_schema, true)?;
                    (format!("Vec<{}>", inner_type), false)
                }
                _ => ("Vec<serde_json::Value>".to_string(), false),
            },
            // Polymorphic types map to generic JSON Value without a discriminator strategy handler elsewhere
            Schema::OneOf(_) | Schema::AnyOf(_) | Schema::AllOf(_) => {
                ("serde_json::Value".to_string(), false)
            }
            _ => ("serde_json::Value".to_string(), false),
        },
    };

    let needs_option = !is_required || nullable;
    if needs_option {
        Ok(format!("Option<{}>", type_str))
    } else {
        Ok(type_str)
    }
}

/// Maps a schema to a Rust type while honoring `contentSchema` for JSON-encoded strings.
///
/// This is used when we still have access to the raw schema JSON, which may include
/// OpenAPI 3.2 keywords (`contentSchema`) not modeled by `utoipa::openapi::Schema`.
pub fn map_schema_to_rust_type_with_raw(
    schema: &RefOr<Schema>,
    is_required: bool,
    raw_schema: Option<&JsonValue>,
) -> AppResult<String> {
    if let Some(raw) = raw_schema {
        if should_use_content_schema(raw) {
            if let Some(content_schema) = content_schema_from_raw(raw) {
                if let Ok(mapped) = map_schema_to_rust_type(&content_schema, is_required) {
                    return Ok(mapped);
                }
            }
        }
    }

    map_schema_to_rust_type(schema, is_required)
}

fn content_schema_from_raw(raw: &JsonValue) -> Option<RefOr<Schema>> {
    let obj = raw.as_object()?;
    let content_schema = obj.get("contentSchema")?;
    serde_json::from_value::<RefOr<Schema>>(content_schema.clone()).ok()
}

fn should_use_content_schema(raw: &JsonValue) -> bool {
    let obj = match raw.as_object() {
        Some(obj) => obj,
        None => return false,
    };

    if !schema_allows_string(obj) {
        return false;
    }

    match obj.get("contentMediaType").and_then(|v| v.as_str()) {
        None => true,
        Some(media) => is_json_media_type(media),
    }
}

fn schema_allows_string(obj: &serde_json::Map<String, JsonValue>) -> bool {
    match obj.get("type") {
        None => true,
        Some(JsonValue::String(t)) => t == "string",
        Some(JsonValue::Array(items)) => items
            .iter()
            .any(|item| item.as_str().map(|s| s == "string").unwrap_or(false)),
        _ => false,
    }
}

fn is_json_media_type(media_type: &str) -> bool {
    let normalized = media_type
        .split(';')
        .next()
        .unwrap_or(media_type)
        .trim()
        .to_ascii_lowercase();
    normalized == "application/json" || normalized.ends_with("+json")
}

fn is_binary_schema(obj: &utoipa::openapi::schema::Object) -> bool {
    if !obj.content_encoding.is_empty() {
        return matches!(obj.content_encoding.as_str(), "base64" | "base64url");
    }

    if obj.content_media_type.is_empty() {
        return false;
    }

    let media = obj.content_media_type.as_str();
    media == "application/octet-stream"
        || media == "application/pdf"
        || media.starts_with("image/")
        || media.starts_with("audio/")
        || media.starts_with("video/")
}

fn normalize_schema_type(schema_type: &SchemaType) -> (SchemaType, bool) {
    match schema_type {
        SchemaType::Array(types) => {
            let mut nullable = false;
            let mut non_null = Vec::new();
            for t in types {
                if *t == Type::Null {
                    nullable = true;
                } else {
                    non_null.push(t.clone());
                }
            }
            if non_null.is_empty() {
                return (SchemaType::AnyValue, true);
            }
            if non_null.len() == 1 {
                return (SchemaType::Type(non_null.remove(0)), nullable);
            }
            (SchemaType::Array(non_null), nullable)
        }
        SchemaType::Type(Type::Null) => (SchemaType::AnyValue, true),
        other => (other.clone(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use utoipa::openapi::schema::{ObjectBuilder, SchemaType, Type};

    #[test]
    fn test_map_primitives() {
        let integer = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::Integer)
                .format(Some(SchemaFormat::KnownFormat(KnownFormat::Int32)))
                .build(),
        );
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(integer), true).unwrap(),
            "i32"
        );

        let long = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::Integer)
                .format(Some(SchemaFormat::KnownFormat(KnownFormat::Int64)))
                .build(),
        );
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(long), true).unwrap(),
            "i64"
        );
    }

    #[test]
    fn test_map_floats_registry() {
        let float = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::Number)
                .format(Some(SchemaFormat::KnownFormat(KnownFormat::Float)))
                .build(),
        );
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(float), true).unwrap(),
            "f32"
        );

        let double = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::Number)
                .format(Some(SchemaFormat::KnownFormat(KnownFormat::Double)))
                .build(),
        );
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(double), true).unwrap(),
            "f64"
        );

        let default_num = Schema::Object(ObjectBuilder::new().schema_type(Type::Number).build());
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(default_num), true).unwrap(),
            "f64"
        );
    }

    #[test]
    fn test_map_strings_registry() {
        let string = Schema::Object(ObjectBuilder::new().schema_type(Type::String).build());
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(string), true).unwrap(),
            "String"
        );

        let uuid = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .format(Some(SchemaFormat::KnownFormat(KnownFormat::Uuid)))
                .build(),
        );
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(uuid), true).unwrap(),
            "Uuid"
        );

        let password = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .format(Some(SchemaFormat::KnownFormat(KnownFormat::Password)))
                .build(),
        );
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(password), true).unwrap(),
            "Secret<String>"
        );
    }

    #[test]
    fn test_map_byte_format_to_vec_u8() {
        let byte = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .format(Some(SchemaFormat::KnownFormat(KnownFormat::Byte)))
                .build(),
        );
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(byte), true).unwrap(),
            "Vec<u8>"
        );
    }

    #[test]
    fn test_map_binary_format_to_vec_u8() {
        let binary = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .format(Some(SchemaFormat::KnownFormat(KnownFormat::Binary)))
                .build(),
        );
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(binary), true).unwrap(),
            "Vec<u8>"
        );
    }

    #[test]
    fn test_map_null_schema() {
        let null_schema = Schema::Object(ObjectBuilder::new().schema_type(Type::Null).build());
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(null_schema), true).unwrap(),
            "Option<serde_json::Value>"
        );
    }

    #[test]
    fn test_map_binary_content_encoding() {
        let bin = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .content_encoding("base64")
                .content_media_type("application/octet-stream")
                .build(),
        );
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(bin), true).unwrap(),
            "Vec<u8>"
        );
    }

    #[test]
    fn test_map_binary_media_type_without_encoding() {
        let bin = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .content_media_type("image/png")
                .build(),
        );
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(bin), true).unwrap(),
            "Vec<u8>"
        );
    }

    #[test]
    fn test_map_text_media_type_stays_string() {
        let text = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .content_media_type("text/plain")
                .build(),
        );
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(text), true).unwrap(),
            "String"
        );
    }

    #[test]
    fn test_formatting_optional() {
        let string = Schema::Object(ObjectBuilder::new().schema_type(Type::String).build());
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(string), false).unwrap(),
            "Option<String>"
        );
    }

    #[test]
    fn test_map_nullable_string_type() {
        let schema = Schema::Object(
            ObjectBuilder::new()
                .schema_type(SchemaType::Array(vec![Type::String, Type::Null]))
                .build(),
        );
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(schema), true).unwrap(),
            "Option<String>"
        );
    }

    #[test]
    fn test_map_union_multi_type_fallback() {
        let schema = Schema::Object(
            ObjectBuilder::new()
                .schema_type(SchemaType::Array(vec![Type::String, Type::Integer]))
                .build(),
        );
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(schema), true).unwrap(),
            "serde_json::Value"
        );
    }

    #[test]
    fn test_ref_resolution() {
        let r = RefOr::Ref(utoipa::openapi::Ref::new("#/components/schemas/User"));
        assert_eq!(map_schema_to_rust_type(&r, true).unwrap(), "User");
    }

    #[test]
    fn test_map_schema_to_rust_type_with_content_schema_json() {
        let schema = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .content_media_type("application/json")
                .build(),
        );
        let raw = json!({
            "type": "string",
            "contentMediaType": "application/json",
            "contentSchema": {
                "type": "integer",
                "format": "int64"
            }
        });

        let mapped = map_schema_to_rust_type_with_raw(&RefOr::T(schema), true, Some(&raw)).unwrap();
        assert_eq!(mapped, "i64");
    }

    #[test]
    fn test_map_schema_to_rust_type_with_content_schema_non_json_ignored() {
        let schema = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .content_media_type("text/plain")
                .build(),
        );
        let raw = json!({
            "type": "string",
            "contentMediaType": "text/plain",
            "contentSchema": {
                "type": "integer"
            }
        });

        let mapped = map_schema_to_rust_type_with_raw(&RefOr::T(schema), true, Some(&raw)).unwrap();
        assert_eq!(mapped, "String");
    }
}
