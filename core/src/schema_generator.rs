#![deny(missing_docs)]

//! # Schema Generator
//!
//! utilities for converting parsed Rust structs into JSON Schema definitions.
//! This module enables the generation of OpenAPI-compliant schemas directly from
//! Rust source code models, respecting Serde attributes like `rename` and `skip`.

use crate::error::AppResult;
use crate::parser::{ParsedField, ParsedStruct};
use serde_json::{json, Map, Value};

/// Generates a JSON Schema object from a parsed Rust struct.
///
/// The resulting schema follows Draft-7 standards suitable for OpenAPI `components/schemas`.
///
/// # Arguments
///
/// * `struct_def` - The parsed struct definition containing fields and attributes.
///
/// # Behavior
///
/// * Maps Rust primitives (`i32`, `String`, `bool`) to JSON Schema types.
/// * Handles `Option<T>` by excluding the field from the `required` list.
/// * Handles `Vec<T>` as `type: array`.
/// * Respects `#[serde(rename = "...")]` for property names.
/// * Skips fields marked with `#[serde(skip)]`.
/// * Includes doc comments as `description`.
///
/// # Returns
///
/// * `AppResult<Value>` - A `serde_json::Value` object representing the schema.
pub fn generate_json_schema(struct_def: &ParsedStruct) -> AppResult<Value> {
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
        // Only valid if using values that can be serialized as JSON strings/values in the array.
        // `required` in JSON Schema is an array of strings.
        let required_json: Vec<Value> = required.into_iter().map(Value::String).collect();
        schema.insert("required".to_string(), Value::Array(required_json));
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
    use crate::parser::ParsedField;

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

    fn make_skipped_field(name: &str, ty: &str) -> ParsedField {
        let mut f = make_field(name, ty, None);
        f.is_skipped = true;
        f
    }

    #[test]
    fn test_generate_simple_schema() {
        let fields = vec![
            make_field("id", "i32", None),
            make_field("active", "bool", None),
        ];

        let def = make_struct("User", fields);
        let schema = generate_json_schema(&def).unwrap();

        let props = schema["properties"].as_object().unwrap();
        assert_eq!(props["id"]["type"], "integer");
        assert_eq!(props["active"]["type"], "boolean");

        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 2);
    }

    #[test]
    fn test_optional_and_vec() {
        let fields = vec![
            make_field("opt", "Option<i32>", None),
            make_field("list", "Vec<String>", None),
        ];

        let def = make_struct("Test", fields);
        let schema = generate_json_schema(&def).unwrap();

        let props = schema["properties"].as_object().unwrap();
        assert_eq!(props["list"]["type"], "array");
        assert_eq!(props["list"]["items"]["type"], "string");

        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1); // Only list is required
        assert_eq!(required[0], "list");
    }

    #[test]
    fn test_skipping_and_renaming() {
        let fields = vec![
            make_field("hidden", "String", Some("visible")),
            make_skipped_field("secret", "String"),
        ];

        let def = make_struct("Secure", fields);
        let schema = generate_json_schema(&def).unwrap();
        let props = schema["properties"].as_object().unwrap();

        assert!(props.contains_key("visible"));
        assert!(!props.contains_key("hidden"));
        assert!(!props.contains_key("secret"));
    }
}
