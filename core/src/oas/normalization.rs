#![deny(missing_docs)]

//! # OpenAPI Normalization
//!
//! Helpers that normalize OpenAPI documents into a more uniform shape before
//! deserializing with `utoipa`. These functions are intentionally conservative
//! and only rewrite fields that are known compatibility gaps.

use serde_json::{json, Map, Value};

/// Normalizes boolean schemas (`true` / `false`) into object schemas.
///
/// OpenAPI 3.1+ permits boolean schemas anywhere a Schema Object is accepted.
/// The `utoipa` OpenAPI model does not deserialize boolean schemas, so we
/// rewrite them into equivalent object schemas before parsing.
///
/// - `true` becomes `{}` (accepts any instance)
/// - `false` becomes an unsatisfiable object schema
pub(crate) fn normalize_boolean_schemas(value: &mut Value) {
    if let Some(components) = value.get_mut("components").and_then(|c| c.as_object_mut()) {
        if let Some(schemas) = components
            .get_mut("schemas")
            .and_then(|s| s.as_object_mut())
        {
            for schema in schemas.values_mut() {
                normalize_schema_node(schema);
            }
        }
    }

    normalize_schema_fields(value);
}

/// Normalizes `nullable` / `x-nullable` schema flags into JSON Schema null unions.
///
/// OpenAPI 3.0 uses `nullable: true` (and Swagger 2.0 often uses `x-nullable: true`).
/// OpenAPI 3.1+ encodes nullability via `type: [T, "null"]`.
///
/// This helper rewrites `nullable` and `x-nullable` into a `type` union where possible,
/// or wraps the schema in `anyOf` when no explicit `type` is present.
pub(crate) fn normalize_nullable_schemas(value: &mut Value) {
    if let Value::Object(map) = value {
        if let Some(replacement) = apply_nullable_flag(map) {
            *value = replacement;
        }
    }

    match value {
        Value::Object(map) => {
            for v in map.values_mut() {
                normalize_nullable_schemas(v);
            }
        }
        Value::Array(items) => {
            for v in items.iter_mut() {
                normalize_nullable_schemas(v);
            }
        }
        _ => {}
    }
}

/// Normalizes JSON Schema `const` usage into single-value `enum` entries.
///
/// This is scoped to schema objects to preserve compatibility with parsers that
/// do not yet recognize the `const` keyword.
pub(crate) fn normalize_const_schemas(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(const_val) = map.remove("const") {
                if !map.contains_key("enum") {
                    map.insert("enum".to_string(), Value::Array(vec![const_val.clone()]));
                }
                if !map.contains_key("type") {
                    if let Some(type_name) = infer_schema_type(&const_val) {
                        map.insert("type".to_string(), Value::String(type_name));
                    }
                }
            }

            for (key, v) in map.iter_mut() {
                if matches!(key.as_str(), "example" | "examples" | "default" | "enum") {
                    continue;
                }
                normalize_const_schemas(v);
            }
        }
        Value::Array(items) => {
            for v in items.iter_mut() {
                normalize_const_schemas(v);
            }
        }
        _ => {}
    }
}

fn infer_schema_type(value: &Value) -> Option<String> {
    match value {
        Value::String(_) => Some("string".to_string()),
        Value::Bool(_) => Some("boolean".to_string()),
        Value::Number(num) => {
            if num.is_i64() || num.is_u64() {
                Some("integer".to_string())
            } else {
                Some("number".to_string())
            }
        }
        Value::Array(_) => Some("array".to_string()),
        Value::Object(_) => Some("object".to_string()),
        Value::Null => Some("null".to_string()),
    }
}

fn normalize_schema_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                match key.as_str() {
                    "schema" | "itemSchema" | "contentSchema" => normalize_schema_node(v),
                    _ => normalize_schema_fields(v),
                }
            }
        }
        Value::Array(items) => {
            for v in items.iter_mut() {
                normalize_schema_fields(v);
            }
        }
        _ => {}
    }
}

fn normalize_schema_node(value: &mut Value) {
    match value {
        Value::Bool(flag) => {
            *value = bool_schema_replacement(*flag);
        }
        Value::Object(map) => {
            if let Some(props) = map.get_mut("properties").and_then(|v| v.as_object_mut()) {
                for v in props.values_mut() {
                    normalize_schema_node(v);
                }
            }
            if let Some(items) = map.get_mut("items") {
                normalize_schema_node(items);
            }
            if let Some(prefix_items) = map.get_mut("prefixItems").and_then(|v| v.as_array_mut()) {
                for v in prefix_items.iter_mut() {
                    normalize_schema_node(v);
                }
            }
            if let Some(all_of) = map.get_mut("allOf").and_then(|v| v.as_array_mut()) {
                for v in all_of.iter_mut() {
                    normalize_schema_node(v);
                }
            }
            if let Some(any_of) = map.get_mut("anyOf").and_then(|v| v.as_array_mut()) {
                for v in any_of.iter_mut() {
                    normalize_schema_node(v);
                }
            }
            if let Some(one_of) = map.get_mut("oneOf").and_then(|v| v.as_array_mut()) {
                for v in one_of.iter_mut() {
                    normalize_schema_node(v);
                }
            }
            if let Some(not_val) = map.get_mut("not") {
                normalize_schema_node(not_val);
            }
            if let Some(contains) = map.get_mut("contains") {
                normalize_schema_node(contains);
            }
            if let Some(prop_names) = map.get_mut("propertyNames") {
                normalize_schema_node(prop_names);
            }
            if let Some(if_val) = map.get_mut("if") {
                normalize_schema_node(if_val);
            }
            if let Some(then_val) = map.get_mut("then") {
                normalize_schema_node(then_val);
            }
            if let Some(else_val) = map.get_mut("else") {
                normalize_schema_node(else_val);
            }
            if let Some(dependent) = map
                .get_mut("dependentSchemas")
                .and_then(|v| v.as_object_mut())
            {
                for v in dependent.values_mut() {
                    normalize_schema_node(v);
                }
            }
            if let Some(additional) = map.get_mut("additionalProperties") {
                if !additional.is_boolean() {
                    normalize_schema_node(additional);
                }
            }
            if let Some(unevaluated) = map.get_mut("unevaluatedProperties") {
                if !unevaluated.is_boolean() {
                    normalize_schema_node(unevaluated);
                }
            }
            if let Some(unevaluated_items) = map.get_mut("unevaluatedItems") {
                if !unevaluated_items.is_boolean() {
                    normalize_schema_node(unevaluated_items);
                }
            }
        }
        Value::Array(items) => {
            for v in items.iter_mut() {
                normalize_schema_node(v);
            }
        }
        _ => {}
    }
}

fn bool_schema_replacement(flag: bool) -> Value {
    if flag {
        Value::Object(Map::new())
    } else {
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["__never__"]
        })
    }
}

fn apply_nullable_flag(map: &mut Map<String, Value>) -> Option<Value> {
    let nullable = map
        .get("nullable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || map
            .get("x-nullable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

    if !nullable {
        return None;
    }

    map.remove("nullable");
    map.remove("x-nullable");

    if let Some(type_val) = map.get_mut("type") {
        match type_val {
            Value::String(s) => {
                if s != "null" {
                    *type_val = Value::Array(vec![
                        Value::String(s.clone()),
                        Value::String("null".to_string()),
                    ]);
                }
            }
            Value::Array(arr) => {
                let has_null = arr.iter().any(|v| v.as_str() == Some("null"));
                if !has_null {
                    arr.push(Value::String("null".to_string()));
                }
            }
            _ => {}
        }
        return None;
    }

    let original = Value::Object(map.clone());
    Some(json!({ "anyOf": [original, { "type": "null" }] }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_normalize_dynamic_ref_preserved() {
        let mut value = json!({
            "$dynamicRef": "#/components/schemas/User"
        });
        normalize_schema_node(&mut value);
        assert_eq!(
            value.get("$dynamicRef").and_then(|v| v.as_str()),
            Some("#/components/schemas/User")
        );
        assert!(value.get("$ref").is_none());
    }

    #[test]
    fn test_normalize_boolean_schemas_in_schema_positions() {
        let mut value = json!({
            "openapi": "3.2.0",
            "components": {
                "schemas": {
                    "Any": true,
                    "Never": false,
                    "WithProps": {
                        "type": "object",
                        "properties": {
                            "flag": true
                        },
                        "additionalProperties": false
                    }
                }
            },
            "paths": {
                "/foo": {
                    "get": {
                        "responses": {
                            "200": {
                                "content": {
                                    "application/json": {
                                        "schema": true
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        normalize_boolean_schemas(&mut value);

        assert!(value["components"]["schemas"]["Any"].is_object());
        assert!(value["components"]["schemas"]["Never"].is_object());
        assert!(value["components"]["schemas"]["WithProps"]["properties"]["flag"].is_object());
        assert!(value["components"]["schemas"]["WithProps"]["additionalProperties"].is_boolean());
        assert!(
            value["paths"]["/foo"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]
                .is_object()
        );
    }
}
