#![deny(missing_docs)]

//! # Type Mapping
//!
//! Logic for mapping OpenAPI Schema definitions to Rust strings.

use crate::error::AppResult;
use utoipa::openapi::schema::{ArrayItems, KnownFormat, Schema, SchemaFormat, SchemaType, Type};
use utoipa::openapi::RefOr;

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
