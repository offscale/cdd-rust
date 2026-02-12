#![deny(missing_docs)]

//! # Schema Generator
//!
//! Utilities for converting parsed Rust structs and enums into JSON Schema definitions.
//! This module enables the generation of OpenAPI-compliant schemas directly from
//! Rust source code models, respecting Serde attributes like `rename`, `rename_all`,
//! `deny_unknown_fields`, `skip`, `tag`, and `untagged`.

use crate::error::AppResult;
use crate::parser::{
    ParsedEnum, ParsedExternalDocs, ParsedField, ParsedModel, ParsedStruct, RenameRule,
};
use serde_json::{json, Map, Value};

/// Minimal OpenAPI Info metadata for OpenAPI document generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiInfo {
    /// The title of the API.
    pub title: String,
    /// A short summary of the API.
    pub summary: Option<String>,
    /// The version of the API document.
    pub version: String,
    /// Optional description for the API.
    pub description: Option<String>,
    /// Optional Terms of Service URL.
    pub terms_of_service: Option<String>,
    /// Optional contact information.
    pub contact: Option<OpenApiContact>,
    /// Optional license information.
    pub license: Option<OpenApiLicense>,
}

impl OpenApiInfo {
    /// Creates a new OpenApiInfo with required fields.
    pub fn new(title: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            summary: None,
            version: version.into(),
            description: None,
            terms_of_service: None,
            contact: None,
            license: None,
        }
    }

    /// Sets an optional summary.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Sets an optional description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the Terms of Service URL.
    pub fn with_terms_of_service(mut self, terms: impl Into<String>) -> Self {
        self.terms_of_service = Some(terms.into());
        self
    }

    /// Sets optional contact metadata.
    pub fn with_contact(mut self, contact: OpenApiContact) -> Self {
        self.contact = Some(contact);
        self
    }

    /// Sets optional license metadata.
    pub fn with_license(mut self, license: OpenApiLicense) -> Self {
        self.license = Some(license);
        self
    }
}

/// Contact metadata for the OpenAPI Info object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiContact {
    /// The identifying name of the contact person/organization.
    pub name: Option<String>,
    /// The URL for the contact information.
    pub url: Option<String>,
    /// The email address of the contact person/organization.
    pub email: Option<String>,
}

impl OpenApiContact {
    /// Creates an empty contact object.
    pub fn new() -> Self {
        Self {
            name: None,
            url: None,
            email: None,
        }
    }

    /// Sets the contact name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the contact URL.
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Sets the contact email.
    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }
}

/// License metadata for the OpenAPI Info object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiLicense {
    /// The license name used for the API.
    pub name: String,
    /// Optional SPDX identifier.
    pub identifier: Option<String>,
    /// Optional URL pointing to the license text.
    pub url: Option<String>,
}

impl OpenApiLicense {
    /// Creates a new license with the required name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            identifier: None,
            url: None,
        }
    }

    /// Sets the license SPDX identifier.
    pub fn with_identifier(mut self, identifier: impl Into<String>) -> Self {
        self.identifier = Some(identifier.into());
        self
    }

    /// Sets the license URL.
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }
}

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

/// Generates a minimal OpenAPI 3.2 document containing the provided model as a component schema.
///
/// This is intended for Rust -> OpenAPI workflows where a single type is being reflected
/// into an OpenAPI description.
pub fn generate_openapi_document(
    model: &ParsedModel,
    dialect: Option<&str>,
    info: &OpenApiInfo,
) -> AppResult<Value> {
    let schema = generate_json_schema(model, dialect)?;

    let mut components = Map::new();
    let mut schemas = Map::new();
    schemas.insert(model.name().to_string(), schema);
    components.insert("schemas".to_string(), Value::Object(schemas));

    let mut info_obj = Map::new();
    info_obj.insert("title".to_string(), json!(info.title));
    info_obj.insert("version".to_string(), json!(info.version));
    if let Some(summary) = &info.summary {
        info_obj.insert("summary".to_string(), json!(summary));
    }
    if let Some(desc) = &info.description {
        info_obj.insert("description".to_string(), json!(desc));
    }
    if let Some(terms) = &info.terms_of_service {
        info_obj.insert("termsOfService".to_string(), json!(terms));
    }
    if let Some(contact) = &info.contact {
        let mut contact_obj = Map::new();
        if let Some(name) = &contact.name {
            contact_obj.insert("name".to_string(), json!(name));
        }
        if let Some(url) = &contact.url {
            contact_obj.insert("url".to_string(), json!(url));
        }
        if let Some(email) = &contact.email {
            contact_obj.insert("email".to_string(), json!(email));
        }
        if !contact_obj.is_empty() {
            info_obj.insert("contact".to_string(), Value::Object(contact_obj));
        }
    }
    if let Some(license) = &info.license {
        let mut license_obj = Map::new();
        license_obj.insert("name".to_string(), json!(license.name));
        if let Some(identifier) = &license.identifier {
            license_obj.insert("identifier".to_string(), json!(identifier));
        }
        if let Some(url) = &license.url {
            license_obj.insert("url".to_string(), json!(url));
        }
        info_obj.insert("license".to_string(), Value::Object(license_obj));
    }

    let mut doc = Map::new();
    doc.insert("openapi".to_string(), json!("3.2.0"));
    if let Some(d) = dialect {
        doc.insert("jsonSchemaDialect".to_string(), json!(d));
    }
    doc.insert("info".to_string(), Value::Object(info_obj));
    doc.insert("components".to_string(), Value::Object(components));

    Ok(Value::Object(doc))
}

/// Generates schema for a struct.
fn generate_struct_schema(struct_def: &ParsedStruct, dialect: Option<&str>) -> AppResult<Value> {
    let mut schema = Map::new();

    // 0. Dialect (if provided)
    if let Some(d) = dialect {
        schema.insert("$schema".to_string(), json!(d));
    }

    // 1. Basic Metadata
    let title = struct_def
        .rename
        .clone()
        .unwrap_or_else(|| struct_def.name.clone());
    schema.insert("title".to_string(), json!(title));

    if let Some(desc) = &struct_def.description {
        schema.insert("description".to_string(), json!(desc));
    }

    if struct_def.is_deprecated {
        schema.insert("deprecated".to_string(), json!(true));
    }
    if let Some(ext) = &struct_def.external_docs {
        schema.insert("externalDocs".to_string(), external_docs_value(ext));
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
            let (_, field_schema, _) = process_field(&field, struct_def.rename_all.as_ref());
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

            let (json_name, mut field_schema, is_optional) =
                process_field(field, struct_def.rename_all.as_ref());

            if field.is_deprecated {
                if let Some(obj) = field_schema.as_object_mut() {
                    obj.insert("deprecated".to_string(), json!(true));
                }
            }
            if let Some(ext) = &field.external_docs {
                if let Some(obj) = field_schema.as_object_mut() {
                    obj.insert("externalDocs".to_string(), external_docs_value(ext));
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

        if struct_def.deny_unknown_fields {
            schema.insert("additionalProperties".to_string(), json!(false));
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

    let title = enum_def
        .rename
        .clone()
        .unwrap_or_else(|| enum_def.name.clone());
    schema.insert("title".to_string(), json!(title));

    if let Some(desc) = &enum_def.description {
        schema.insert("description".to_string(), json!(desc));
    }

    if enum_def.is_deprecated {
        schema.insert("deprecated".to_string(), json!(true));
    }
    if let Some(ext) = &enum_def.external_docs {
        schema.insert("externalDocs".to_string(), external_docs_value(ext));
    }

    let mut one_of = Vec::new();

    for variant in &enum_def.variants {
        let variant_name = variant.rename.clone().unwrap_or_else(|| {
            enum_def
                .rename_all
                .as_ref()
                .map(|rule| rule.apply(&variant.name))
                .unwrap_or_else(|| variant.name.clone())
        });

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
        // Tagged enum: add discriminator hint (with optional mapping)
        let mut discriminator = Map::new();
        discriminator.insert("propertyName".to_string(), json!(tag));
        if let Some(mapping) = &enum_def.discriminator_mapping {
            if !mapping.is_empty() {
                discriminator.insert("mapping".to_string(), json!(mapping));
            }
        }
        if let Some(default_mapping) = &enum_def.discriminator_default_mapping {
            if !default_mapping.is_empty() {
                discriminator.insert("defaultMapping".to_string(), json!(default_mapping));
            }
        }
        schema.insert("discriminator".to_string(), Value::Object(discriminator));
    }

    Ok(Value::Object(schema))
}

fn external_docs_value(ext: &ParsedExternalDocs) -> Value {
    let mut map = Map::new();
    map.insert("url".to_string(), json!(ext.url));
    if let Some(desc) = &ext.description {
        map.insert("description".to_string(), json!(desc));
    }
    Value::Object(map)
}

/// Processes a single field to determine its JSON name, schema, and optionality.
fn process_field(field: &ParsedField, rename_all: Option<&RenameRule>) -> (String, Value, bool) {
    // 1. Determine Name
    let name = field
        .rename
        .clone()
        .or_else(|| rename_all.map(|rule| rule.apply(&field.name)))
        .unwrap_or_else(|| field.name.clone());

    // 2. Parse Type (special-case binary payloads)
    let (mut schema, is_optional) = if is_binary_type(&field.ty) {
        (binary_schema_value(), is_optional_type(&field.ty))
    } else {
        let parsed = parse_rust_type(&field.ty);
        let schema = shape_to_schema(&parsed.shape);
        (schema, parsed.is_optional)
    };

    if is_optional {
        schema = make_nullable_schema(schema);
    }

    // 5. Add Description
    if let Some(desc) = &field.description {
        if let Some(obj) = schema.as_object_mut() {
            obj.insert("description".to_string(), json!(desc));
        }
    }

    (name, schema, is_optional)
}

fn binary_schema_value() -> Value {
    json!({
        "type": "string",
        "contentEncoding": "base64",
        "contentMediaType": "application/octet-stream"
    })
}

fn make_nullable_schema(schema: Value) -> Value {
    let Some(obj) = schema.as_object() else {
        return json!({ "anyOf": [schema, { "type": "null" }] });
    };

    if obj.contains_key("$ref") {
        return json!({ "anyOf": [schema, { "type": "null" }] });
    }

    if let Some(type_val) = obj.get("type") {
        let mut types = match type_val {
            Value::String(s) => vec![Value::String(s.clone())],
            Value::Array(arr) => arr.clone(),
            _ => Vec::new(),
        };

        if !types
            .iter()
            .any(|t| matches!(t, Value::String(s) if s == "null"))
        {
            types.push(Value::String("null".to_string()));
        }

        let mut new_obj = obj.clone();
        new_obj.insert("type".to_string(), Value::Array(types));
        return Value::Object(new_obj);
    }

    json!({ "anyOf": [schema, { "type": "null" }] })
}

fn is_optional_type(ty: &str) -> bool {
    let ty = ty.trim();
    ty.starts_with("Option<") && ty.ends_with('>')
}

fn is_binary_type(ty: &str) -> bool {
    let ty = ty.trim();
    if is_optional_type(ty) {
        let inner = &ty[7..ty.len() - 1];
        return is_binary_type(inner);
    }

    if ty.starts_with("Vec<") && ty.ends_with('>') {
        let inner = ty[4..ty.len() - 1].trim();
        return inner == "u8";
    }

    if ty == "&[u8]" || ty == "[u8]" {
        return true;
    }

    ty == "Bytes" || ty == "ByteBuf" || ty.ends_with("::Bytes") || ty.ends_with("::ByteBuf")
}

/// Parses the Rust type string to identify wrappers like `Option<...>` and `Vec<...>`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeShape {
    Base(String),
    Vec(Box<TypeShape>),
    Map(Box<TypeShape>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedType {
    shape: TypeShape,
    is_optional: bool,
}

/// Parses the Rust type string to identify wrappers like `Option<...>`, `Vec<...>`,
/// and map-like containers (`HashMap`/`BTreeMap`).
fn parse_rust_type(ty: &str) -> ParsedType {
    parse_rust_type_inner(ty.trim(), true)
}

fn parse_rust_type_inner(ty: &str, allow_optional: bool) -> ParsedType {
    if allow_optional {
        if let Some(inner) = strip_generic(ty, "Option") {
            let parsed = parse_rust_type_inner(inner, false);
            return ParsedType {
                shape: parsed.shape,
                is_optional: true,
            };
        }
    }

    if let Some(inner) = strip_generic(ty, "Vec") {
        let parsed = parse_rust_type_inner(inner, false);
        return ParsedType {
            shape: TypeShape::Vec(Box::new(parsed.shape)),
            is_optional: false,
        };
    }

    if let Some(map_inner) = strip_map_generic(ty) {
        let parsed = parse_rust_type_inner(map_inner.as_str(), false);
        return ParsedType {
            shape: TypeShape::Map(Box::new(parsed.shape)),
            is_optional: false,
        };
    }

    ParsedType {
        shape: TypeShape::Base(ty.to_string()),
        is_optional: false,
    }
}

fn strip_generic<'a>(ty: &'a str, target: &str) -> Option<&'a str> {
    let (base, inner) = split_generic(ty)?;
    if base == target {
        Some(inner)
    } else {
        None
    }
}

fn strip_map_generic(ty: &str) -> Option<String> {
    let (base, inner) = split_generic(ty)?;
    if base == "HashMap" || base == "BTreeMap" {
        let args = split_generic_args(inner);
        if args.len() == 2 {
            return Some(args[1].trim().to_string());
        }
    }
    None
}

fn split_generic<'a>(ty: &'a str) -> Option<(&'a str, &'a str)> {
    let start = ty.find('<')?;
    if !ty.ends_with('>') {
        return None;
    }
    let base = ty[..start].trim();
    let base = base.split("::").last().unwrap_or(base);
    let inner = &ty[start + 1..ty.len() - 1];
    Some((base, inner))
}

fn split_generic_args(inner: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();

    for ch in inner.chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                if depth > 0 {
                    depth -= 1;
                }
                current.push(ch);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }

    args
}

fn shape_to_schema(shape: &TypeShape) -> Value {
    match shape {
        TypeShape::Base(ty) => map_type_to_schema(ty),
        TypeShape::Vec(inner) => json!({
            "type": "array",
            "items": shape_to_schema(inner)
        }),
        TypeShape::Map(inner) => json!({
            "type": "object",
            "additionalProperties": shape_to_schema(inner)
        }),
    }
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
    use std::collections::BTreeMap;

    fn make_struct(name: &str, fields: Vec<ParsedField>) -> ParsedStruct {
        ParsedStruct {
            name: name.into(),
            description: Some("Test Struct".into()),
            rename: None,
            rename_all: None,
            fields,
            is_deprecated: false,
            deny_unknown_fields: false,
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
    fn test_generate_schema_rename_all_and_deny_unknown() {
        let fields = vec![
            make_field("user_id", "i32", None),
            make_field("display_name", "String", None),
        ];
        let mut s = make_struct("UserProfile", fields);
        s.rename_all = Some(RenameRule::CamelCase);
        s.deny_unknown_fields = true;

        let schema = generate_json_schema(&ParsedModel::Struct(s), None).unwrap();
        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("userId"));
        assert!(props.contains_key("displayName"));
        assert_eq!(schema["additionalProperties"], false);
    }

    #[test]
    fn test_generate_openapi_document_wraps_schema() {
        let fields = vec![make_field("id", "i32", None)];
        let def = ParsedModel::Struct(make_struct("User", fields));
        let contact = OpenApiContact::new()
            .with_name("API Support")
            .with_email("support@example.com");
        let license = OpenApiLicense::new("Apache 2.0").with_identifier("Apache-2.0");
        let info = OpenApiInfo::new("Test API", "1.0.0")
            .with_summary("Short summary")
            .with_description("Docs")
            .with_terms_of_service("https://example.com/terms")
            .with_contact(contact)
            .with_license(license);
        let dialect = "https://spec.openapis.org/oas/3.1/dialect/base";

        let doc = generate_openapi_document(&def, Some(dialect), &info).unwrap();
        assert_eq!(doc["openapi"], "3.2.0");
        assert_eq!(doc["jsonSchemaDialect"], dialect);
        assert_eq!(doc["info"]["title"], "Test API");
        assert_eq!(doc["info"]["version"], "1.0.0");
        assert_eq!(doc["info"]["summary"], "Short summary");
        assert_eq!(doc["info"]["description"], "Docs");
        assert_eq!(doc["info"]["termsOfService"], "https://example.com/terms");
        assert_eq!(doc["info"]["contact"]["name"], "API Support");
        assert_eq!(doc["info"]["contact"]["email"], "support@example.com");
        assert_eq!(doc["info"]["license"]["name"], "Apache 2.0");
        assert_eq!(doc["info"]["license"]["identifier"], "Apache-2.0");
        assert_eq!(doc["components"]["schemas"]["User"]["title"], "User");
    }

    #[test]
    fn test_generate_map_schema() {
        let fields = vec![make_field("tags", "HashMap<String, i32>", None)];
        let def = ParsedModel::Struct(make_struct("Tagged", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        let props = schema["properties"].as_object().unwrap();
        let tags = &props["tags"];
        assert_eq!(tags["type"], "object");
        assert_eq!(tags["additionalProperties"]["type"], "integer");
    }

    #[test]
    fn test_generate_nested_map_schema() {
        let fields = vec![make_field("meta", "BTreeMap<String, Vec<String>>", None)];
        let def = ParsedModel::Struct(make_struct("Meta", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        let props = schema["properties"].as_object().unwrap();
        let meta = &props["meta"];
        assert_eq!(meta["type"], "object");
        assert_eq!(meta["additionalProperties"]["type"], "array");
        assert_eq!(meta["additionalProperties"]["items"]["type"], "string");
    }

    #[test]
    fn test_generate_external_docs_on_schema_and_field() {
        let mut fields = vec![make_field("id", "i32", None)];
        fields[0].external_docs = Some(ParsedExternalDocs {
            url: "https://example.com/field".to_string(),
            description: Some("Field docs".to_string()),
        });

        let mut s = make_struct("DocUser", fields);
        s.external_docs = Some(ParsedExternalDocs {
            url: "https://example.com/schema".to_string(),
            description: Some("Schema docs".to_string()),
        });

        let schema = generate_json_schema(&ParsedModel::Struct(s), None).unwrap();
        assert_eq!(schema["externalDocs"]["url"], "https://example.com/schema");
        assert_eq!(schema["externalDocs"]["description"], "Schema docs");

        let props = schema["properties"].as_object().unwrap();
        let id = &props["id"];
        assert_eq!(id["externalDocs"]["url"], "https://example.com/field");
        assert_eq!(id["externalDocs"]["description"], "Field docs");
    }

    #[test]
    fn test_optional_map_not_required() {
        let fields = vec![make_field(
            "labels",
            "Option<HashMap<String, String>>",
            None,
        )];
        let def = ParsedModel::Struct(make_struct("OptionalMap", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        assert!(schema.get("required").is_none());
        let labels = &schema["properties"]["labels"];
        let types = labels["type"].as_array().unwrap();
        assert!(types
            .iter()
            .any(|t| matches!(t, Value::String(s) if s == "null")));
    }

    #[test]
    fn test_optional_string_nullable_schema() {
        let fields = vec![make_field("nickname", "Option<String>", None)];
        let def = ParsedModel::Struct(make_struct("NullableUser", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        let nickname = &schema["properties"]["nickname"];
        let types = nickname["type"].as_array().unwrap();
        assert!(types
            .iter()
            .any(|t| matches!(t, Value::String(s) if s == "null")));
        assert!(types
            .iter()
            .any(|t| matches!(t, Value::String(s) if s == "string")));
    }

    #[test]
    fn test_optional_ref_schema_uses_anyof() {
        let fields = vec![make_field("owner", "Option<User>", None)];
        let def = ParsedModel::Struct(make_struct("Owned", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        let owner = &schema["properties"]["owner"];
        assert!(owner.get("anyOf").is_some());
    }

    #[test]
    fn test_generate_enum_schema() {
        let en = ParsedEnum {
            name: "Pet".into(),
            description: None,
            rename: None,
            rename_all: None,
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
            discriminator_default_mapping: None,
        };

        let schema = generate_json_schema(&ParsedModel::Enum(en), None).unwrap();
        assert!(schema["oneOf"].is_array());
        assert!(schema["discriminator"].is_object());
        assert_eq!(schema["discriminator"]["propertyName"], "type");
        assert!(!schema.as_object().unwrap().contains_key("$schema"));
    }

    #[test]
    fn test_generate_enum_schema_with_rename_all() {
        let en = ParsedEnum {
            name: "Color".into(),
            description: None,
            rename: None,
            rename_all: Some(RenameRule::KebabCase),
            tag: None,
            untagged: true,
            is_deprecated: false,
            external_docs: None,
            variants: vec![
                ParsedVariant {
                    name: "RedBlue".into(),
                    ty: None,
                    description: None,
                    rename: None,
                    aliases: None,
                    is_deprecated: false,
                },
                ParsedVariant {
                    name: "Green".into(),
                    ty: None,
                    description: None,
                    rename: None,
                    aliases: None,
                    is_deprecated: false,
                },
            ],
            discriminator_mapping: None,
            discriminator_default_mapping: None,
        };

        let schema = generate_json_schema(&ParsedModel::Enum(en), None).unwrap();
        let one_of = schema["oneOf"].as_array().unwrap();
        assert_eq!(one_of[0]["const"], "red-blue");
        assert_eq!(one_of[1]["const"], "green");
    }

    #[test]
    fn test_generate_struct_title_uses_rename() {
        let fields = vec![make_field("id", "i32", None)];
        let mut s = make_struct("User", fields);
        s.rename = Some("UserModel".to_string());

        let schema = generate_json_schema(&ParsedModel::Struct(s), None).unwrap();
        assert_eq!(schema["title"], "UserModel");
    }

    #[test]
    fn test_generate_enum_schema_with_discriminator_mapping() {
        let mut mapping = BTreeMap::new();
        mapping.insert("cat".to_string(), "#/components/schemas/Cat".to_string());
        mapping.insert("dog".to_string(), "#/components/schemas/Dog".to_string());

        let en = ParsedEnum {
            name: "Pet".into(),
            description: None,
            rename: None,
            rename_all: None,
            tag: Some("kind".into()),
            untagged: false,
            is_deprecated: false,
            external_docs: None,
            variants: vec![],
            discriminator_mapping: Some(mapping.clone()),
            discriminator_default_mapping: Some("OtherPet".to_string()),
        };

        let schema = generate_json_schema(&ParsedModel::Enum(en), None).unwrap();
        assert_eq!(schema["discriminator"]["propertyName"], "kind");
        assert_eq!(schema["discriminator"]["mapping"], json!(mapping));
        assert_eq!(schema["discriminator"]["defaultMapping"], "OtherPet");
    }

    #[test]
    fn test_generate_enum_schema_with_dialect() {
        let en = ParsedEnum {
            name: "Status".into(),
            description: None,
            rename: None,
            rename_all: None,
            tag: None,
            untagged: false,
            is_deprecated: false,
            external_docs: None,
            variants: vec![],
            discriminator_mapping: None,
            discriminator_default_mapping: None,
        };
        let dialect = "https://json-schema.org/draft/2020-12/schema";
        let schema = generate_json_schema(&ParsedModel::Enum(en), Some(dialect)).unwrap();
        assert_eq!(schema["$schema"], dialect);
        assert_eq!(schema["title"], "Status");
    }

    #[test]
    fn test_generate_binary_vec_u8_schema() {
        let fields = vec![make_field("payload", "Vec<u8>", None)];
        let def = ParsedModel::Struct(make_struct("Upload", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        let payload = &schema["properties"]["payload"];
        assert_eq!(payload["type"], "string");
        assert_eq!(payload["contentEncoding"], "base64");
        assert_eq!(payload["contentMediaType"], "application/octet-stream");
    }

    #[test]
    fn test_generate_binary_bytes_schema() {
        let fields = vec![make_field("data", "bytes::Bytes", None)];
        let def = ParsedModel::Struct(make_struct("Blob", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        let data = &schema["properties"]["data"];
        assert_eq!(data["type"], "string");
        assert_eq!(data["contentEncoding"], "base64");
    }

    #[test]
    fn test_generate_binary_optional_not_required() {
        let fields = vec![make_field("payload", "Option<Vec<u8>>", None)];
        let def = ParsedModel::Struct(make_struct("Upload", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        assert!(schema.get("required").is_none());
    }
}
