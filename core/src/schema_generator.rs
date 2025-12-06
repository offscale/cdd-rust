#![deny(missing_docs)]

//! # Schema Generator
//!
//! Utilities for converting parsed Rust structs and enums into JSON Schema definitions.
//! This module enables the generation of OpenAPI-compliant schemas directly from
//! Rust source code models, respecting Serde attributes like `rename`, `skip`, `tag`, and `untagged`.

use crate::error::AppResult;
use crate::parser::{ParsedEnum, ParsedField, ParsedModel, ParsedStruct};
use serde_json::{json, Map, Value};

/// Generates a JSON Schema object from a parsed Rust model (struct or enum).
pub fn generate_json_schema(model: &ParsedModel) -> AppResult<Value> {
    match model {
        ParsedModel::Struct(s) => generate_struct_schema(s),
        ParsedModel::Enum(e) => generate_enum_schema(e),
    }
}

/// Generates schema for a struct.
fn generate_struct_schema(struct_def: &ParsedStruct) -> AppResult<Value> {
    let mut schema = Map::new();

    // 1. Basic Metadata
    schema.insert("title".to_string(), json!(struct_def.name));
    schema.insert("type".to_string(), json!("object"));

    if let Some(desc) = &struct_def.description {
        schema.insert("description".to_string(), json!(desc));
    }

    // 2. Properties & Required
    let mut properties = Map::new();
    let mut required = Vec::new();

    for field in &struct_def.fields {
        if field.is_skipped {
            continue;
        }

        let (json_name, field_schema, is_optional) = process_field(field);

        properties.insert(json_name.clone(), field_schema);

        if !is_optional {
            required.push(json_name);
        }
    }

    schema.insert("properties".to_string(), Value::Object(properties));

    if !required.is_empty() {
        let required_json: Vec<Value> = required.into_iter().map(Value::String).collect();
        schema.insert("required".to_string(), Value::Array(required_json));
    }

    Ok(Value::Object(schema))
}

/// Generates schema for an enum using `oneOf`.
fn generate_enum_schema(enum_def: &ParsedEnum) -> AppResult<Value> {
    let mut schema = Map::new();
    schema.insert("title".to_string(), json!(enum_def.name));

    if let Some(desc) = &enum_def.description {
        schema.insert("description".to_string(), json!(desc));
    }

    let mut one_of = Vec::new();

    for variant in &enum_def.variants {
        let variant_name = variant
            .rename
            .clone()
            .unwrap_or_else(|| variant.name.clone());

        // Determine variant schema
        // If variant wraps a type: Variant(Type)
        let sub_schema = if let Some(ty) = &variant.ty {
            map_type_to_schema(ty)
        } else {
            // Unit variant -> Enum::A -> "A"
            json!({ "type": "string", "const": variant_name })
        };

        one_of.push(sub_schema);
    }

    // Handle Untagged vs Tagged
    schema.insert("oneOf".to_string(), Value::Array(one_of));

    if let Some(tag) = &enum_def.tag {
        // Tagged enum: add discriminator hint
        let discriminator = json!({
            "propertyName": tag
        });
        schema.insert("discriminator".to_string(), discriminator);
    }

    Ok(Value::Object(schema))
}

/// Processes a single field to determine its JSON name, schema, and optionality.
fn process_field(field: &ParsedField) -> (String, Value, bool) {
    // 1. Determine Name
    let name = field.rename.clone().unwrap_or_else(|| field.name.clone());

    // 2. Parse Type
    let (inner_type, is_optional, is_array) = parse_rust_type(&field.ty);

    // 3. Map to JSON Schema Type
    let mut schema = map_type_to_schema(&inner_type);

    // 4. Wrap in Array if Vector
    if is_array {
        schema = json!({
            "type": "array",
            "items": schema
        });
    }

    // 5. Add Description
    if let Some(desc) = &field.description {
        if let Some(obj) = schema.as_object_mut() {
            obj.insert("description".to_string(), json!(desc));
        }
    }

    (name, schema, is_optional)
}

/// Parses the Rust type string to identify wrappers like `Option<...>` and `Vec<...>`.
fn parse_rust_type(ty: &str) -> (String, bool, bool) {
    let ty = ty.trim();

    // Check Option
    if ty.starts_with("Option<") && ty.ends_with('>') {
        let inner = &ty[7..ty.len() - 1];
        let (deep_inner, _, is_vec) = parse_rust_type(inner);
        return (deep_inner, true, is_vec);
    }

    // Check Vec
    if ty.starts_with("Vec<") && ty.ends_with('>') {
        let inner = &ty[4..ty.len() - 1];
        let (deep_inner, _, _) = parse_rust_type(inner);
        return (deep_inner, false, true);
    }

    (ty.to_string(), false, false)
}

/// Maps a "clean" Rust type to a base JSON Schema object.
fn map_type_to_schema(ty: &str) -> Value {
    match ty {
        "i8" | "i16" | "i32" | "u8" | "u16" | "u32" | "isize" | "usize" => {
            json!({ "type": "integer", "format": "int32" })
        }
        "i64" | "u64" => json!({ "type": "integer", "format": "int64" }),

        "f32" | "f64" => json!({ "type": "number" }),

        "bool" => json!({ "type": "boolean" }),

        "String" | "&str" | "char" => json!({ "type": "string" }),
        "Uuid" => json!({ "type": "string", "format": "uuid" }),
        "NaiveDate" => json!({ "type": "string", "format": "date" }),
        "NaiveDateTime" | "DateTime<Utc>" | "DateTime<Local>" => {
            json!({ "type": "string", "format": "date-time" })
        }
        "Decimal" => json!({ "type": "string", "format": "decimal" }),

        // Fallback
        other => json!({ "$ref": format!("#/components/schemas/{}", other) }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParsedField, ParsedVariant};

    fn make_struct(name: &str, fields: Vec<ParsedField>) -> ParsedStruct {
        ParsedStruct {
            name: name.into(),
            description: Some("Test Struct".into()),
            rename: None,
            fields,
        }
    }

    fn make_field(name: &str, ty: &str, rename: Option<&str>) -> ParsedField {
        ParsedField {
            name: name.into(),
            ty: ty.into(),
            description: Some("A field".into()),
            is_skipped: false,
            rename: rename.map(|s| s.into()),
        }
    }

    #[test]
    fn test_generate_simple_schema() {
        let fields = vec![
            make_field("id", "i32", None),
            make_field("active", "bool", None),
        ];

        let def = ParsedModel::Struct(make_struct("User", fields));
        let schema = generate_json_schema(&def).unwrap();

        let props = schema["properties"].as_object().unwrap();
        assert_eq!(props["id"]["type"], "integer");
        assert_eq!(props["active"]["type"], "boolean");
    }

    #[test]
    fn test_generate_enum_schema() {
        let en = ParsedEnum {
            name: "Pet".into(),
            description: None,
            rename: None,
            tag: Some("type".into()),
            untagged: false,
            variants: vec![
                ParsedVariant {
                    name: "Cat".into(),
                    ty: Some("CatInfo".into()),
                    description: None,
                    rename: None,
                    aliases: None,
                },
                ParsedVariant {
                    name: "Dog".into(),
                    ty: Some("DogInfo".into()),
                    description: None,
                    rename: None,
                    aliases: None,
                },
            ],
        };

        let schema = generate_json_schema(&ParsedModel::Enum(en)).unwrap();
        assert!(schema["oneOf"].is_array());
        assert!(schema["discriminator"].is_object());
        assert_eq!(schema["discriminator"]["propertyName"], "type");
    }
}
