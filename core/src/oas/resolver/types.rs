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
use utoipa::openapi::schema::{ArrayItems, KnownFormat, Schema, SchemaFormat, SchemaType, Type};
use utoipa::openapi::RefOr;

/// Maps an OpenAPI Schema definition to a Rust type string.
///
/// # Arguments
///
/// * `schema` - The schema definition (Ref or Object).
/// * `is_required` - Whether the field is mandatory (wraps in `Option` if false).
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
                    Some(SchemaFormat::KnownFormat(KnownFormat::Double)) => "f64".to_string(),
                    // Default for number without format is f64 in Rust
                    _ => "f64".to_string(),
                },
                SchemaType::Type(Type::Boolean) => "bool".to_string(),
                SchemaType::Type(Type::String) => match &obj.format {
                    Some(SchemaFormat::KnownFormat(KnownFormat::Uuid)) => "Uuid".to_string(),
                    Some(SchemaFormat::KnownFormat(KnownFormat::DateTime)) => {
                        "DateTime".to_string()
                    }
                    Some(SchemaFormat::KnownFormat(KnownFormat::Date)) => "NaiveDate".to_string(),
                    Some(SchemaFormat::KnownFormat(KnownFormat::Password)) => {
                        "Secret<String>".to_string()
                    }
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
            // Polymorphic types map to generic JSON Value without a discriminator strategy handler elsewhere
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

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::openapi::schema::{ObjectBuilder, Type};

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
    fn test_formatting_optional() {
        let string = Schema::Object(ObjectBuilder::new().schema_type(Type::String).build());
        assert_eq!(
            map_schema_to_rust_type(&RefOr::T(string), false).unwrap(),
            "Option<String>"
        );
    }

    #[test]
    fn test_ref_resolution() {
        let r = RefOr::Ref(utoipa::openapi::Ref::new("#/components/schemas/User"));
        assert_eq!(map_schema_to_rust_type(&r, true).unwrap(), "User");
    }
}
