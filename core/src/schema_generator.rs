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
///
/// # Arguments
///
/// * `model` - The parsed Rust model.
/// * `dialect` - Optional JSON Schema dialect URI to associate with the schema.
pub fn generate_json_schema(model: &ParsedModel, dialect: Option<&str>) -> AppResult<Value> {
    match model {
        ParsedModel::Struct(s) => generate_struct_schema(s, dialect),
        ParsedModel::Enum(e) => generate_enum_schema(e, dialect),
    }
}

/// Generates schema for a struct.
fn generate_struct_schema(struct_def: &ParsedStruct, dialect: Option<&str>) -> AppResult<Value> {
    let mut schema = Map::new();

    // 0. Dialect (if provided)
    if let Some(d) = dialect {
        schema.insert("$schema".to_string(), json!(d));
    }

    // 1. Basic Metadata
    schema.insert("title".to_string(), json!(struct_def.name));

    if let Some(desc) = &struct_def.description {
        schema.insert("description".to_string(), json!(desc));
    }

    if struct_def.is_deprecated {
        schema.insert("deprecated".to_string(), json!(true));
    }

    // 2. Check if it is a Tuple Struct vs Named Struct
    // A Tuple struct in Rust is parsed with fields named "0", "1", "2"...
    // We detect if ALL fields are numeric.
    let is_tuple = !struct_def.fields.is_empty()
        && struct_def
            .fields
            .iter()
            .all(|f| f.name.chars().all(char::is_numeric));

    if is_tuple {
        schema.insert("type".to_string(), json!("array"));
        let mut prefix_items = Vec::new();

        // Sort fields by index to ensure correct order
        let mut sorted_fields = struct_def.fields.clone();
        sorted_fields.sort_by_key(|f| f.name.parse::<usize>().unwrap_or(0));

        for field in sorted_fields {
            if field.is_skipped {
                continue;
            }
            let (_, field_schema, _) = process_field(&field);
            prefix_items.push(field_schema);
        }

        schema.insert("prefixItems".to_string(), Value::Array(prefix_items));
        // Tuple structs are fixed length in Rust
        schema.insert("items".to_string(), json!(false));
    } else {
        schema.insert("type".to_string(), json!("object"));

        let mut properties = Map::new();
        let mut required = Vec::new();

        for field in &struct_def.fields {
            if field.is_skipped {
                continue;
            }

            let (json_name, mut field_schema, is_optional) = process_field(field);

            if field.is_deprecated {
                if let Some(obj) = field_schema.as_object_mut() {
                    obj.insert("deprecated".to_string(), json!(true));
                }
            }

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
    }

    Ok(Value::Object(schema))
}

/// Generates schema for an enum using `oneOf`.
fn generate_enum_schema(enum_def: &ParsedEnum, dialect: Option<&str>) -> AppResult<Value> {
    let mut schema = Map::new();

    // 0. Dialect
    if let Some(d) = dialect {
        schema.insert("$schema".to_string(), json!(d));
    }

    schema.insert("title".to_string(), json!(enum_def.name));

    if let Some(desc) = &enum_def.description {
        schema.insert("description".to_string(), json!(desc));
    }

    if enum_def.is_deprecated {
        schema.insert("deprecated".to_string(), json!(true));
    }

    let mut one_of = Vec::new();

    for variant in &enum_def.variants {
        let variant_name = variant
            .rename
            .clone()
            .unwrap_or_else(|| variant.name.clone());

        // Determine variant schema
        let mut sub_schema = if let Some(ty) = &variant.ty {
            map_type_to_schema(ty)
        } else {
            // Unit variant -> Enum::A -> "A"
            json!({ "type": "string", "const": variant_name })
        };

        if variant.is_deprecated {
            if let Some(obj) = sub_schema.as_object_mut() {
                obj.insert("deprecated".to_string(), json!(true));
            }
        }

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
            is_deprecated: false,
            external_docs: None,
        }
    }

    fn make_field(name: &str, ty: &str, rename: Option<&str>) -> ParsedField {
        ParsedField {
            name: name.into(),
            ty: ty.into(),
            description: Some("A field".into()),
            is_skipped: false,
            rename: rename.map(|s| s.into()),
            is_deprecated: false,
            external_docs: None,
        }
    }

    #[test]
    fn test_generate_simple_schema() {
        let fields = vec![
            make_field("id", "i32", None),
            make_field("active", "bool", None),
        ];

        let def = ParsedModel::Struct(make_struct("User", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        let props = schema["properties"].as_object().unwrap();
        assert_eq!(props["id"]["type"], "integer");
        assert_eq!(props["active"]["type"], "boolean");
        assert!(!schema.as_object().unwrap().contains_key("$schema"));
    }

    #[test]
    fn test_generate_tuple_schema() {
        let fields = vec![
            make_field("0", "i32", None),
            make_field("1", "String", None),
        ];

        let def = ParsedModel::Struct(make_struct("Point", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        assert_eq!(schema["type"], "array");
        assert_eq!(schema["items"], false);
        let prefix_items = schema["prefixItems"].as_array().unwrap();
        assert_eq!(prefix_items.len(), 2);
        assert_eq!(prefix_items[0]["type"], "integer");
        assert_eq!(prefix_items[1]["type"], "string");
    }

    #[test]
    fn test_generate_schema_with_dialect() {
        let fields = vec![make_field("id", "i32", None)];
        let def = ParsedModel::Struct(make_struct("User", fields));
        let dialect = "https://spec.openapis.org/oas/3.1/dialect/base";

        let schema = generate_json_schema(&def, Some(dialect)).unwrap();

        assert_eq!(schema["$schema"], dialect);
        assert_eq!(schema["title"], "User");
    }

    #[test]
    fn test_generate_enum_schema() {
        let en = ParsedEnum {
            name: "Pet".into(),
            description: None,
            rename: None,
            tag: Some("type".into()),
            untagged: false,
            is_deprecated: false,
            external_docs: None,
            variants: vec![
                ParsedVariant {
                    name: "Cat".into(),
                    ty: Some("CatInfo".into()),
                    description: None,
                    rename: None,
                    aliases: None,
                    is_deprecated: false,
                },
                ParsedVariant {
                    name: "Dog".into(),
                    ty: Some("DogInfo".into()),
                    description: None,
                    rename: None,
                    aliases: None,
                    is_deprecated: false,
                },
            ],
            discriminator_mapping: None,
        };

        let schema = generate_json_schema(&ParsedModel::Enum(en), None).unwrap();
        assert!(schema["oneOf"].is_array());
        assert!(schema["discriminator"].is_object());
        assert_eq!(schema["discriminator"]["propertyName"], "type");
        assert!(!schema.as_object().unwrap().contains_key("$schema"));
    }

    #[test]
    fn test_generate_enum_schema_with_dialect() {
        let en = ParsedEnum {
            name: "Status".into(),
            description: None,
            rename: None,
            tag: None,
            untagged: false,
            is_deprecated: false,
            external_docs: None,
            variants: vec![],
            discriminator_mapping: None,
        };
        let dialect = "https://json-schema.org/draft/2020-12/schema";
        let schema = generate_json_schema(&ParsedModel::Enum(en), Some(dialect)).unwrap();
        assert_eq!(schema["$schema"], dialect);
        assert_eq!(schema["title"], "Status");
    }
}
