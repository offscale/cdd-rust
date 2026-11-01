#![deny(missing_docs)]

//! # OpenAPI Normalization
//!
//! Helpers that normalize OpenAPI documents into a more uniform shape before
//! deserializing with `utoipa`. These functions are intentionally conservative
//! and only rewrite fields that are known compatibility gaps.

use serde_json::{json, Map, Value};

/// Upgrades a Google Discovery REST description document (`discovery#restDescription`)
/// into a canonical OpenAPI 3.2.0 document in-place.
///
/// # Arguments
/// * `value` - Mutable JSON value representing the parsed specification document.
pub(crate) fn upgrade_google_discovery_to_openapi3(value: &mut Value) {
    let Some(root) = value.as_object_mut() else {
        return;
    };
    let is_discovery = root.contains_key("discoveryVersion")
        || root.get("kind").and_then(Value::as_str) == Some("discovery#restDescription");
    if !is_discovery {
        return;
    }

    let title = root
        .remove("title")
        .or_else(|| root.remove("name"))
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "Discovery API".to_string());
    let version = root
        .remove("version")
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "v1".to_string());
    let description = root
        .remove("description")
        .and_then(|v| v.as_str().map(str::to_string));

    let root_url = root
        .remove("rootUrl")
        .and_then(|v| v.as_str().map(str::to_string));
    let service_path = root
        .remove("servicePath")
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();

    let schemas = root.remove("schemas");
    let resources = root.remove("resources");
    let top_methods = root.remove("methods");
    root.remove("discoveryVersion");
    root.remove("kind");
    root.remove("name");
    root.remove("id");
    root.remove("protocol");
    root.remove("baseUrl");
    root.remove("basePath");
    root.remove("batchPath");
    root.remove("parameters");
    root.remove("auth");

    root.insert("openapi".to_string(), Value::String("3.2.0".to_string()));
    let mut info = Map::new();
    info.insert("title".to_string(), Value::String(title));
    info.insert("version".to_string(), Value::String(version));
    if let Some(desc) = description {
        info.insert("description".to_string(), Value::String(desc));
    }
    root.insert("info".to_string(), Value::Object(info));

    if let Some(r_url) = root_url {
        let base = r_url.trim_end_matches('/');
        let sp = service_path.trim_matches('/');
        let full_url = if sp.is_empty() {
            base.to_string()
        } else {
            format!("{base}/{sp}")
        };
        root.insert("servers".to_string(), json!([{ "url": full_url }]));
    }

    if let Some(mut s_val) = schemas {
        rewrite_discovery_refs(&mut s_val);
        root.insert("components".to_string(), json!({ "schemas": s_val }));
    }

    let mut paths: std::collections::BTreeMap<String, Map<String, Value>> =
        std::collections::BTreeMap::new();
    if let Some(Value::Object(m_map)) = top_methods {
        collect_discovery_methods(&m_map, &mut paths);
    }
    if let Some(Value::Object(res_map)) = resources {
        collect_discovery_resources(&res_map, &mut paths);
    }
    let paths_val: Map<String, Value> = paths
        .into_iter()
        .map(|(k, v)| (k, Value::Object(v)))
        .collect();
    root.insert("paths".to_string(), Value::Object(paths_val));
}

/// Recursively rewrites bare Google Discovery `$ref` identifiers into `#/components/schemas/<Name>`.
///
/// # Arguments
/// * `value` - Input `value` parameter.
fn rewrite_discovery_refs(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(r)) = map.get_mut("$ref") {
                if !r.starts_with('#') && !r.contains('/') {
                    *r = format!("#/components/schemas/{r}");
                }
            }
            for v in map.values_mut() {
                rewrite_discovery_refs(v);
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_discovery_refs(item);
            }
        }
        _ => {}
    }
}

/// Recursively traverses Google Discovery `resources` blocks to extract operations into `paths`.
///
/// # Arguments
/// * `res_map` - Input `res_map` parameter.
/// * `paths` - Input `paths` parameter.
fn collect_discovery_resources(
    res_map: &Map<String, Value>,
    paths: &mut std::collections::BTreeMap<String, Map<String, Value>>,
) {
    for res_val in res_map.values() {
        if let Some(res_obj) = res_val.as_object() {
            if let Some(Value::Object(methods)) = res_obj.get("methods") {
                collect_discovery_methods(methods, paths);
            }
            if let Some(Value::Object(sub_res)) = res_obj.get("resources") {
                collect_discovery_resources(sub_res, paths);
            }
        }
    }
}

/// Converts a Google Discovery `methods` map into OpenAPI Path Item operations.
///
/// # Arguments
/// * `methods` - Input `methods` parameter.
/// * `paths` - Input `paths` parameter.
fn collect_discovery_methods(
    methods: &Map<String, Value>,
    paths: &mut std::collections::BTreeMap<String, Map<String, Value>>,
) {
    for (method_key, method_val) in methods {
        let Some(m_obj) = method_val.as_object() else {
            continue;
        };
        let raw_path = m_obj
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or(method_key.as_str());
        let path_key = format!("/{}", raw_path.trim_start_matches('/'));
        let verb = m_obj
            .get("httpMethod")
            .and_then(Value::as_str)
            .unwrap_or("GET")
            .to_ascii_lowercase();
        let op_id = m_obj
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(method_key.as_str())
            .replace('.', "_");

        let mut op = Map::new();
        op.insert("operationId".to_string(), Value::String(op_id));
        if let Some(Value::String(desc)) = m_obj.get("description") {
            op.insert("description".to_string(), Value::String(desc.clone()));
        }

        if let Some(Value::Object(params_map)) = m_obj.get("parameters") {
            let mut params_arr = Vec::new();
            for (p_name, p_val) in params_map {
                if let Some(p_obj) = p_val.as_object() {
                    let loc = p_obj
                        .get("location")
                        .and_then(Value::as_str)
                        .unwrap_or("query");
                    let required = loc == "path"
                        || p_obj
                            .get("required")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                    let p_type = p_obj
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("string");
                    params_arr.push(json!({
                        "name": p_name,
                        "in": loc,
                        "required": required,
                        "schema": { "type": p_type }
                    }));
                }
            }
            if !params_arr.is_empty() {
                op.insert("parameters".to_string(), Value::Array(params_arr));
            }
        }

        if let Some(req_ref) = m_obj
            .get("request")
            .and_then(|r| r.get("$ref"))
            .and_then(Value::as_str)
        {
            op.insert(
                "requestBody".to_string(),
                json!({
                    "required": true,
                    "content": {
                        "application/json": {
                            "schema": { "$ref": format!("#/components/schemas/{req_ref}") }
                        }
                    }
                }),
            );
        }

        let resp_val = if let Some(resp_ref) = m_obj
            .get("response")
            .and_then(|r| r.get("$ref"))
            .and_then(Value::as_str)
        {
            json!({
                "200": {
                    "description": "OK",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": format!("#/components/schemas/{resp_ref}") }
                        }
                    }
                }
            })
        } else {
            json!({
                "200": {
                    "description": "OK"
                }
            })
        };
        op.insert("responses".to_string(), resp_val);

        paths
            .entry(path_key)
            .or_default()
            .insert(verb, Value::Object(op));
    }
}

/// Normalizes boolean schemas (`true` / `false`) into object schemas.
///
/// OpenAPI 3.1+ permits boolean schemas anywhere a Schema Object is accepted.
/// The `utoipa` OpenAPI model does not deserialize boolean schemas, so we
/// rewrite them into equivalent object schemas before parsing.
///
/// - `true` becomes `{}` (accepts any instance)
/// - `false` becomes an unsatisfiable object schema
///
/// # Arguments
/// * `value` - Input `value` parameter.
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
///
/// # Arguments
/// * `value` - Input `value` parameter.
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
///
/// # Arguments
/// * `value` - Input `value` parameter.
pub(crate) fn normalize_const_schemas(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(const_val) = map.remove("const") {
                if !map.contains_key("enum") {
                    map.insert("enum".to_string(), Value::Array(vec![const_val.clone()]));
                }
                if !map.contains_key("type") {
                    let type_name = infer_schema_type(&const_val);
                    map.insert("type".to_string(), Value::String(type_name.to_string()));
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

/// Infers a JSON Schema `type` string from a JSON literal value.
///
/// # Arguments
/// * `value` - Input `value` parameter.
///
/// # Returns
/// Resulting `&'static str` value.
fn infer_schema_type(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Bool(_) => "boolean",
        Value::Number(num) => {
            if num.is_i64() || num.is_u64() {
                "integer"
            } else {
                "number"
            }
        }
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "null",
    }
}

/// Traverses an OpenAPI JSON tree and normalizes boolean schemas inside `schema`, `itemSchema`, and `contentSchema` fields.
///
/// # Arguments
/// * `value` - Input `value` parameter.
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

/// Normalizes a single schema node (converting booleans to object schemas and recursing into subschemas).
///
/// # Arguments
/// * `value` - Input `value` parameter.
fn normalize_schema_node(value: &mut Value) {
    match value {
        Value::Bool(flag) => {
            *value = bool_schema_replacement(*flag);
        }
        Value::Object(map) => {
            if map.contains_key("properties") && !map.contains_key("type") {
                map.insert("type".to_string(), Value::String("object".to_string()));
            }

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

/// Converts a boolean schema literal (`true` or `false`) into its equivalent object schema representation.
///
/// # Arguments
/// * `flag` - Input `flag` parameter.
///
/// # Returns
/// Resulting `Value` value.
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

/// Rewrites `nullable: true` or `x-nullable: true` on a schema map into a `type` union or `anyOf` wrapper.
///
/// # Arguments
/// * `map` - Input `map` parameter.
///
/// # Returns
/// Resulting `Option<Value>` value.
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
            Value::String(s) if s != "null" => {
                *type_val = Value::Array(vec![
                    Value::String(s.clone()),
                    Value::String("null".to_string()),
                ]);
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

        // Cover normalize_nullable_schemas, normalize_const_schemas, and remaining branches of normalize_boolean_schemas
        let mut nullable_val = json!({
            "s1": { "nullable": true, "type": "string" },
            "s2": { "nullable": true, "type": ["string", "null"] },
            "s3": { "nullable": true, "type": ["integer"] },
            "s4": { "nullable": true, "type": 123 },
            "s5": { "nullable": true },
            "s6": { "nullable": false, "type": "boolean" },
            "s7": { "nullable": "not_a_bool", "type": "string" },
            "arr": [ { "nullable": true, "type": "number" }, "prim" ]
        });
        normalize_nullable_schemas(&mut nullable_val);
        assert_eq!(nullable_val["s1"]["type"], json!(["string", "null"]));
        assert_eq!(nullable_val["s2"]["type"], json!(["string", "null"]));
        assert_eq!(nullable_val["s3"]["type"], json!(["integer", "null"]));
        assert!(nullable_val["s5"].get("nullable").is_none());

        let mut const_val = json!({
            "c_str": { "const": "hello" },
            "c_int": { "const": 42 },
            "c_u64": { "const": 18446744073709551615u64 },
            "c_flt": { "const": 3.5 },
            "c_bool": { "const": true },
            "c_arr": { "const": [1, 2] },
            "c_obj": { "const": {"a": 1} },
            "c_null": { "const": null },
            "c_with_enum": { "const": "x", "enum": ["x"] },
            "c_with_type": { "const": "y", "type": "string" },
            "example": { "const": "skip" },
            "arr": [ { "const": 1 }, 99 ]
        });
        normalize_const_schemas(&mut const_val);
        assert_eq!(const_val["c_str"]["enum"], json!(["hello"]));
        assert_eq!(const_val["c_str"]["type"], json!("string"));
        assert_eq!(const_val["c_int"]["type"], json!("integer"));
        assert_eq!(const_val["c_flt"]["type"], json!("number"));
        assert_eq!(const_val["c_bool"]["type"], json!("boolean"));
        assert_eq!(const_val["c_arr"]["type"], json!("array"));
        assert_eq!(const_val["c_obj"]["type"], json!("object"));
        assert_eq!(const_val["c_null"]["type"], json!("null"));

        let mut bool_edge = json!({
            "arr": [
                {
                    "itemSchema": {
                        "properties": { "p": true },
                        "items": [true, "prim"],
                        "prefixItems": [false],
                        "allOf": [true],
                        "anyOf": [false],
                        "oneOf": [true],
                        "not": false,
                        "contains": true,
                        "propertyNames": true,
                        "if": true,
                        "then": true,
                        "else": false,
                        "dependentSchemas": { "d": true },
                        "additionalProperties": true,
                        "unevaluatedProperties": false,
                        "unevaluatedItems": false
                    },
                    "contentSchema": {
                        "additionalProperties": { "not": true },
                        "unevaluatedProperties": { "not": true },
                        "unevaluatedItems": { "not": true }
                    },
                    "schema": {
                        "additionalProperties": false,
                        "unevaluatedProperties": false,
                        "unevaluatedItems": false
                    }
                },
                "prim"
            ]
        });
        normalize_boolean_schemas(&mut bool_edge);
        assert_eq!(bool_edge["arr"][0]["itemSchema"]["type"], json!("object"));
        assert!(bool_edge["arr"][0]["contentSchema"]["additionalProperties"]["not"].is_object());

        let mut x_null = json!({
            "x-nullable": true,
            "type": "null"
        });
        normalize_nullable_schemas(&mut x_null);
        assert_eq!(x_null["type"], json!("null"));

        let mut non_obj = json!("not an object");
        upgrade_google_discovery_to_openapi3(&mut non_obj);
        assert_eq!(non_obj, json!("not an object"));

        let mut discovery = json!({
            "discoveryVersion": "v1",
            "kind": "discovery#restDescription",
            "name": "petstore",
            "version": "v1",
            "title": "Petstore Discovery",
            "description": "Google Discovery Petstore",
            "rootUrl": "https://petstore.googleapis.com/",
            "servicePath": "v1/",
            "schemas": {
                "Pet": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "friend": { "$ref": "Pet" },
                        "tags": { "type": "array", "items": [{ "$ref": "Pet" }, { "$ref": "#/components/schemas/Existing" }] }
                    }
                }
            },
            "methods": {
                "ping": {
                    "path": "ping",
                    "httpMethod": "GET",
                    "description": "Health ping"
                },
                "invalid": "not_an_object"
            },
            "resources": {
                "pets": {
                    "methods": {
                        "create": {
                            "id": "petstore.pets.create",
                            "path": "/pets/{petId}",
                            "httpMethod": "POST",
                            "parameters": {
                                "petId": { "type": "string", "location": "path", "required": true },
                                "filter": { "type": "string", "location": "query" },
                                "ignored": "not_an_object"
                            },
                            "request": { "$ref": "Pet" },
                            "response": { "$ref": "Pet" }
                        }
                    },
                    "resources": {
                        "sub": {
                            "methods": {
                                "list": {}
                            }
                        },
                        "empty_sub": {}
                    }
                },
                "bad_res": "not_an_object"
            }
        });
        upgrade_google_discovery_to_openapi3(&mut discovery);
        assert_eq!(discovery["openapi"], json!("3.2.0"));
        assert_eq!(discovery["info"]["title"], json!("Petstore Discovery"));
        assert_eq!(
            discovery["servers"][0]["url"],
            json!("https://petstore.googleapis.com/v1")
        );
        assert_eq!(
            discovery["components"]["schemas"]["Pet"]["properties"]["friend"]["$ref"],
            json!("#/components/schemas/Pet")
        );
        assert!(discovery["paths"]["/pets/{petId}"]["post"].is_object());
        assert!(discovery["paths"]["/ping"]["get"].is_object());

        let mut discovery_min = json!({
            "kind": "discovery#restDescription",
            "rootUrl": "https://example.com/"
        });
        upgrade_google_discovery_to_openapi3(&mut discovery_min);
        assert_eq!(discovery_min["info"]["title"], json!("Discovery API"));
        assert_eq!(
            discovery_min["servers"][0]["url"],
            json!("https://example.com")
        );

        let mut discovery_name_only = json!({
            "discoveryVersion": "v1",
            "name": "my_service",
            "schemas": {
                "Ext": { "$ref": "folder/Schema" }
            },
            "methods": {
                "empty_params": {
                    "parameters": {}
                }
            }
        });
        upgrade_google_discovery_to_openapi3(&mut discovery_name_only);
        assert_eq!(discovery_name_only["info"]["title"], json!("my_service"));
        assert!(discovery_name_only.get("servers").is_none());
    }
}
