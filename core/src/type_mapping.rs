#![deny(missing_docs)]

//! # Type Mapping
//!
//! Converts Rust types into a JSON Schema compatible representation.
//! Handles primitives, collections (Vec), and nullability (Option).
//!
//! Updated for OpenAPI 3.2 compliance regarding Binary Data (`contentMediaType`)
//! and Format Registry (`password`, `float`, `double`).

use crate::error::{AppError, AppResult};
// Import HasGenericArgs to access .generic_arg_list() on PathSegments
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasGenericArgs};
use ra_ap_syntax::{AstNode, SourceFile};
use serde_json::{json, Value};
use std::fmt::Display;

/// Represents the simplified JSON types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonType {
    /// A string type.
    String,
    /// An integer type.
    Integer,
    /// A floating point number.
    Number,
    /// A boolean type.
    Boolean,
    /// An array containing items of a specific schema.
    Array(Box<JsonSchema>),
    /// An object map with dynamic string keys and typed values (`additionalProperties`).
    Map(Box<JsonSchema>),
    /// A reference to another named schema (e.g., a Struct).
    Ref(String),
}

impl Display for JsonType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonType::String => write!(f, "string"),
            JsonType::Integer => write!(f, "integer"),
            JsonType::Number => write!(f, "number"),
            JsonType::Boolean => write!(f, "boolean"),
            JsonType::Array(inner) => write!(f, "array<{}>", inner.type_),
            JsonType::Map(inner) => write!(f, "map<{}>", inner.type_),
            JsonType::Ref(s) => write!(f, "$ref:{}", s),
        }
    }
}

/// Represents the schema definition for a mapped type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonSchema {
    /// The primary JSON type.
    pub type_: JsonType,
    /// Optional format specifier (e.g., "uuid", "date-time", "password").
    pub format: Option<String>,
    /// Whether the field can be null (derived from `Option<T>`).
    pub nullable: bool,
    /// Media type for string-encoded binary data (OAS 3.2).
    /// e.g. "application/octet-stream"
    pub content_media_type: Option<String>,
    /// Encoding for string-encoded binary data (OAS 3.2).
    /// e.g. "base64"
    pub content_encoding: Option<String>,
}

/// Trait for converting Rust type strings to JSON Schemas.
pub trait TypeMapper {
    /// Maps a Rust type string (e.g., `Option<i32>`) to a JSON Schema.
    ///
    /// # Arguments
    /// * `rust_type` - Input `rust_type` parameter.
    ///
    /// # Returns
    /// Resulting `AppResult<JsonSchema>` value.
    ///
    /// # Errors
    /// Returns an error if validation, parsing, or execution fails.
    fn map(&self, rust_type: &str) -> AppResult<JsonSchema>;
}

/// A standard implementation of `TypeMapper`.
pub struct RustToJsonMapper;

impl TypeMapper for RustToJsonMapper {
    fn map(&self, rust_type: &str) -> AppResult<JsonSchema> {
        let trimmed = rust_type.trim();
        if trimmed.is_empty() {
            return Err(AppError::General("Empty type string".into()));
        }
        // Wrap implementation to parse valid Rust syntax using a type alias
        let code = format!("type _Wrapper = {};", trimmed);
        let parse = SourceFile::parse(&code, Edition::Edition2021);
        let file = parse.tree();

        // Extract the type node from the type alias definition
        let root_type = file
            .syntax()
            .descendants()
            .find_map(ast::TypeAlias::cast)
            .and_then(|ta| ta.ty())
            .ok_or_else(|| AppError::General(format!("Invalid type syntax: {}", rust_type)))?;

        // Recursively map the AST node
        map_ast_type(&root_type)
    }
}

/// Recursively maps an AST Type node to JsonSchema.
///
/// # Arguments
/// * `ty` - Input `ty` parameter.
///
/// # Returns
/// Resulting `AppResult<JsonSchema>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
fn map_ast_type(ty: &ast::Type) -> AppResult<JsonSchema> {
    match ty {
        ast::Type::PathType(path_type) => {
            let (segment, name_ref) = path_type
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| {
                    let nr = s.name_ref()?;
                    Some((s, nr))
                })
                .ok_or_else(|| AppError::General("Empty path segment".into()))?;
            let name = name_ref.text();

            match name {
                // Primitives
                "String" | "str" | "char" => Ok(simple(JsonType::String)),
                "bool" => Ok(simple(JsonType::Boolean)),
                "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
                | "u128" | "usize" => Ok(simple(JsonType::Integer)),

                // Numbers (Registry support)
                "f32" => Ok(formatted(JsonType::Number, "float")),
                "f64" => Ok(formatted(JsonType::Number, "double")),

                // Complex / Formats
                "Uuid" => Ok(formatted(JsonType::String, "uuid")),
                "NaiveDateTime" | "DateTime" => Ok(formatted(JsonType::String, "date-time")),
                "NaiveDate" => Ok(formatted(JsonType::String, "date")),
                "Decimal" => Ok(formatted(JsonType::String, "decimal")),

                // Binary types (explicit Bytes wrapper)
                // Vec<u8> is handled via container generic check below, but bytes::Bytes is direct.
                "Bytes" | "ByteBuf" => Ok(binary_schema()),

                // Containers & Wrappers
                "Box" => handle_generic_wrapper(segment, |inner| inner),
                "Option" => handle_generic_wrapper(segment, |mut inner| {
                    inner.nullable = true;
                    inner
                }),
                "Vec" => handle_generic_wrapper(segment, |inner| {
                    // Special case: Vec<u8> -> Binary (OAS 3.2 best practice)
                    // If the inner type is strictly an integer U8, we return binary string.
                    // This avoids generating array<integer> for byte arrays.
                    if inner.type_ == JsonType::Integer && is_u8_heuristic(ty) {
                        return binary_schema();
                    }

                    JsonSchema {
                        type_: JsonType::Array(Box::new(inner)),
                        format: None,
                        nullable: false,
                        content_media_type: None,
                        content_encoding: None,
                    }
                }),
                "HashMap" | "BTreeMap" => handle_map_wrapper(segment),
                // Secret<T> wrapper (Registry: format: password)
                "Secret" => handle_generic_wrapper(segment, |mut inner| {
                    // Only apply password format if inner is a String
                    if inner.type_ == JsonType::String {
                        inner.format = Some("password".to_string());
                    }
                    inner
                }),

                // Fallback: User defined structs or unknown types become References
                other => Ok(simple(JsonType::Ref(other.to_string()))),
            }
        }
        // Correct AST variant for reference types (e.g. &str, &[u8])
        ast::Type::RefType(ref_type) => {
            let inner = ref_type
                .ty()
                .ok_or_else(|| AppError::General("Invalid reference".into()))?;
            map_ast_type(&inner)
        }
        ast::Type::SliceType(slice) => {
            let inner = slice
                .ty()
                .ok_or_else(|| AppError::General("Invalid slice type".into()))?;
            let inner_schema = map_ast_type(&inner)?;
            if inner.to_string().trim() == "u8" {
                return Ok(binary_schema());
            }
            Ok(JsonSchema {
                type_: JsonType::Array(Box::new(inner_schema)),
                format: None,
                nullable: false,
                content_media_type: None,
                content_encoding: None,
            })
        }
        _ => Err(AppError::General(format!(
            "Unsupported type structure: {:?}",
            ty
        ))),
    }
}

/// Helper to handle types like `Option<T>` or `Vec<T>`.
///
/// # Arguments
/// * `segment` - Input `segment` parameter.
/// * `wrap` - Input `wrap` parameter.
///
/// # Returns
/// Resulting `AppResult<JsonSchema> where F: FnOnce(JsonSchema) -> JsonSchema,` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
fn handle_generic_wrapper<F>(segment: ast::PathSegment, wrap: F) -> AppResult<JsonSchema>
where
    F: FnOnce(JsonSchema) -> JsonSchema,
{
    let first_arg = segment
        .generic_arg_list()
        .and_then(|list| list.generic_args().next())
        .ok_or_else(|| AppError::General("Missing generic arguments for container type".into()))?;

    let inner_ty = match first_arg {
        ast::GenericArg::TypeArg(type_arg) => type_arg.ty(),
        _ => None,
    }
    .ok_or_else(|| AppError::General("Unsupported generic argument type".into()))?;

    let schema = map_ast_type(&inner_ty)?;
    Ok(wrap(schema))
}

/// Helper to handle key-value map types like `HashMap<K, V>` and `BTreeMap<K, V>`.
///
/// # Arguments
/// * `segment` - Input `segment` parameter.
///
/// # Returns
/// Resulting `AppResult<JsonSchema>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
fn handle_map_wrapper(segment: ast::PathSegment) -> AppResult<JsonSchema> {
    let mut args = segment
        .generic_arg_list()
        .ok_or_else(|| AppError::General("Missing generic arguments for map type".into()))?
        .generic_args();
    let _key_arg = args
        .next()
        .ok_or_else(|| AppError::General("Missing map key generic argument".into()))?;
    let val_arg = args
        .next()
        .ok_or_else(|| AppError::General("Missing map value generic argument".into()))?;

    let val_ty = match val_arg {
        ast::GenericArg::TypeArg(type_arg) => type_arg.ty(),
        _ => None,
    }
    .ok_or_else(|| AppError::General("Unsupported map value generic argument".into()))?;

    let inner = map_ast_type(&val_ty)?;
    Ok(JsonSchema {
        type_: JsonType::Map(Box::new(inner)),
        format: None,
        nullable: false,
        content_media_type: None,
        content_encoding: None,
    })
}

/// Maps a Rust field type string into an OpenAPI 3.2 JSON Schema [`Value`] and an `is_optional` flag.
///
/// # Arguments
/// * `ty` - The Rust type string (e.g., `"Option<Vec<Uuid>>"`, `"&[u8]"`).
///
/// # Returns
/// * `(Value, bool)` containing the JSON Schema object and `true` if the outer type is `Option<T>`.
pub fn map_rust_type_to_openapi_schema(ty: &str) -> (Value, bool) {
    let (mut schema, is_optional) = map_rust_type_to_unwrapped_openapi_schema(ty);

    if is_optional {
        schema = make_nullable_schema(schema);
    }

    (schema, is_optional)
}

/// Maps a Rust type string into its inner (non-nullable) OpenAPI 3.2 JSON Schema [`Value`] and an `is_optional` flag.
///
/// # Arguments
/// * `ty` - The Rust type string (e.g., `"Option<String>"`, `"Vec<u8>"`).
///
/// # Returns
/// * `(Value, bool)` containing the unwrapped JSON Schema object and `true` if the outer type is `Option<T>`.
pub fn map_rust_type_to_unwrapped_openapi_schema(ty: &str) -> (Value, bool) {
    if is_binary_type(ty) {
        (binary_schema_value(), is_optional_type(ty))
    } else {
        let parsed = parse_rust_type(ty);
        let schema = shape_to_schema(&parsed.shape);
        (schema, parsed.is_optional)
    }
}

/// Maps a base (unwrapped) Rust type name to an OpenAPI 3.2 JSON Schema [`Value`].
///
/// # Arguments
/// * `ty` - Base Rust type identifier (e.g., `"i32"`, `"Uuid"`, `"Pet"`).
///
/// # Returns
/// * JSON Schema [`Value`] representing the type or a `$ref` to `#/components/schemas/{ty}`.
pub fn map_base_type_to_openapi_schema(ty: &str) -> Value {
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
        other => json!({ "$ref": format!("#/components/schemas/{}", other) }),
    }
}

/// Executes `binary_schema_value` (binary schema value).
///
/// # Returns
/// Resulting `Value` value.
fn binary_schema_value() -> Value {
    json!({
        "type": "string",
        "contentEncoding": "base64",
        "contentMediaType": "application/octet-stream"
    })
}

/// Executes `make_nullable_schema` (make nullable schema).
///
/// # Arguments
/// * `schema` - Input `schema` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn make_nullable_schema(schema: Value) -> Value {
    let obj = schema.as_object().expect("schema object");
    if let Some(Value::String(s)) = obj.get("type") {
        let mut new_obj = obj.clone();
        new_obj.insert("type".to_string(), json!([s, "null"]));
        return Value::Object(new_obj);
    }

    json!({ "anyOf": [schema, { "type": "null" }] })
}

/// Executes `is_optional_type` (is optional type).
///
/// # Arguments
/// * `ty` - Input `ty` parameter.
///
/// # Returns
/// Resulting `bool` value.
fn is_optional_type(ty: &str) -> bool {
    let ty = ty.trim();
    ty.starts_with("Option<") && ty.ends_with('>')
}

/// Executes `is_binary_type` (is binary type).
///
/// # Arguments
/// * `ty` - Input `ty` parameter.
///
/// # Returns
/// Resulting `bool` value.
fn is_binary_type(ty: &str) -> bool {
    let ty = ty.trim();
    if is_optional_type(ty) {
        let inner = &ty[7..ty.len() - 1];
        return is_binary_type(inner);
    }

    if let Some(inner) = strip_generic(ty, "Vec") {
        return inner.trim() == "u8";
    }

    matches!(ty, "&[u8]" | "[u8]" | "Bytes" | "ByteBuf")
        || ["::Bytes", "::ByteBuf"].iter().any(|s| ty.ends_with(s))
}

/// Enumeration representing `TypeShape` (type shape).
#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeShape {
    /// Variant `Base` (base).
    Base(String),
    /// Variant `Vec` (vec).
    Vec(Box<TypeShape>),
    /// Variant `Map` (map).
    Map(Box<TypeShape>),
}

/// Internal representation for `ParsedType` (parsed type).
#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedType {
    /// Field `shape` (shape).
    shape: TypeShape,
    /// Field `is_optional` (is optional).
    is_optional: bool,
}

/// Executes `parse_rust_type` (parse rust type).
///
/// # Arguments
/// * `ty` - Input `ty` parameter.
///
/// # Returns
/// Resulting `ParsedType` value.
fn parse_rust_type(ty: &str) -> ParsedType {
    parse_rust_type_inner(ty.trim(), true)
}

/// Executes `parse_rust_type_inner` (parse rust type inner).
///
/// # Arguments
/// * `ty` - Input `ty` parameter.
/// * `allow_optional` - Input `allow_optional` parameter.
///
/// # Returns
/// Resulting `ParsedType` value.
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

/// Executes `strip_generic` (strip generic).
///
/// # Arguments
/// * `ty` - Input `ty` parameter.
/// * `target` - Input `target` parameter.
///
/// # Returns
/// Resulting `Option<&'a str>` value.
fn strip_generic<'a>(ty: &'a str, target: &str) -> Option<&'a str> {
    let (base, inner) = split_generic(ty)?;
    if base == target {
        Some(inner)
    } else {
        None
    }
}

/// Executes `strip_map_generic` (strip map generic).
///
/// # Arguments
/// * `ty` - Input `ty` parameter.
///
/// # Returns
/// Resulting `Option<String>` value.
fn strip_map_generic(ty: &str) -> Option<String> {
    let (base, inner) = split_generic(ty)?;
    if matches!(base, "HashMap" | "BTreeMap") {
        let args = split_generic_args(inner);
        if args.len() == 2 {
            return Some(args[1].trim().to_string());
        }
    }
    None
}

/// Executes `split_generic` (split generic).
///
/// # Arguments
/// * `ty` - Input `ty` parameter.
///
/// # Returns
/// Resulting `Option<(&str, &str)>` value.
fn split_generic(ty: &str) -> Option<(&str, &str)> {
    let start = ty.find('<')?;
    if !ty.ends_with('>') {
        return None;
    }
    let base = ty[..start].trim();
    let base = base.split("::").last().unwrap_or(base);
    let inner = &ty[start + 1..ty.len() - 1];
    Some((base, inner))
}

/// Executes `split_generic_args` (split generic args).
///
/// # Arguments
/// * `inner` - Input `inner` parameter.
///
/// # Returns
/// Resulting `Vec<String>` value.
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
                depth = depth.saturating_sub(1);
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

/// Executes `shape_to_schema` (shape to schema).
///
/// # Arguments
/// * `shape` - Input `shape` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn shape_to_schema(shape: &TypeShape) -> Value {
    match shape {
        TypeShape::Base(ty) => map_base_type_to_openapi_schema(ty),
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

// Helpers for cleaner construction
/// Executes `simple` (simple).
///
/// # Arguments
/// * `t` - Input `t` parameter.
///
/// # Returns
/// Resulting `JsonSchema` value.
fn simple(t: JsonType) -> JsonSchema {
    JsonSchema {
        type_: t,
        format: None,
        nullable: false,
        content_media_type: None,
        content_encoding: None,
    }
}

/// Executes `formatted` (formatted).
///
/// # Arguments
/// * `t` - Input `t` parameter.
/// * `fmt` - Input `fmt` parameter.
///
/// # Returns
/// Resulting `JsonSchema` value.
fn formatted(t: JsonType, fmt: &str) -> JsonSchema {
    JsonSchema {
        type_: t,
        format: Some(fmt.to_string()),
        nullable: false,
        content_media_type: None,
        content_encoding: None,
    }
}

/// Helper to generate the OAS 3.2 compliant binary schema.
/// `type: string`, `contentMediaType: application/octet-stream`, `contentEncoding: base64`.
fn binary_schema() -> JsonSchema {
    JsonSchema {
        type_: JsonType::String,
        format: None, // Legacy "byte" omitted in favor of content* fields, or can be added if desired.
        nullable: false,
        content_media_type: Some("application/octet-stream".to_string()),
        content_encoding: Some("base64".to_string()),
    }
}

/// Checks via [`crate::openapi::parse::models::RustType`] whether `ty` represents `Vec<u8>`.
///
/// # Arguments
/// * `ty` - Input `ty` parameter.
///
/// # Returns
/// Resulting `bool` value.
fn is_u8_heuristic(ty: &ast::Type) -> bool {
    let text = ty.to_string();
    crate::openapi::parse::models::RustType::parse(&text)
        == crate::openapi::parse::models::RustType::Bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_mapping() {
        let mapper = RustToJsonMapper;

        let cases = vec![
            ("i32", JsonType::Integer),
            // ("f32", JsonType::Number), // Now formatted
            ("bool", JsonType::Boolean),
            ("String", JsonType::String),
        ];

        for (input, expected) in cases {
            let res = mapper.map(input).expect(input);
            assert_eq!(res.type_, expected);
        }
    }

    #[test]
    fn test_float_precision_mapping() {
        let mapper = RustToJsonMapper;

        let f32_res = mapper.map("f32").expect("Failed to map");
        assert_eq!(f32_res.type_, JsonType::Number);
        assert_eq!(f32_res.format.as_deref(), Some("float"));

        let f64_res = mapper.map("f64").expect("Failed to map");
        assert_eq!(f64_res.type_, JsonType::Number);
        assert_eq!(f64_res.format.as_deref(), Some("double"));
    }

    #[test]
    fn test_secret_wrapper_mapping() {
        let mapper = RustToJsonMapper;

        // Secret<String> -> format: password
        let secret_str = mapper.map("Secret<String>").expect("Failed to map");
        assert_eq!(secret_str.type_, JsonType::String);
        assert_eq!(secret_str.format.as_deref(), Some("password"));

        // Secret<i32> -> no password format (applies to strings)
        let secret_int = mapper.map("Secret<i32>").expect("Failed to map");
        assert_eq!(secret_int.type_, JsonType::Integer);
        assert_eq!(secret_int.format, None);
    }

    #[test]
    fn test_references() {
        let mapper = RustToJsonMapper;
        // &str -> string
        let res = mapper.map("&str").expect("should map ref");
        assert_eq!(res.type_, JsonType::String);
    }

    #[test]
    fn test_options_nullable() {
        let mapper = RustToJsonMapper;
        let res = mapper.map("Option<i32>").expect("Failed to map");
        assert!(res.nullable);
        assert_eq!(res.type_, JsonType::Integer);
    }

    #[test]
    fn test_nested_complex() {
        let mapper = RustToJsonMapper;
        let res = mapper.map("Option<Vec<Uuid>>").expect("Failed to map");

        assert!(res.nullable);
        assert_eq!(
            res.type_,
            JsonType::Array(Box::new(JsonSchema {
                type_: JsonType::String,
                format: Some("uuid".to_string()),
                nullable: false,
                content_media_type: None,
                content_encoding: None,
            }))
        );

        let vec_str = mapper.map("Vec<String>").expect("Vec<String>");
        assert_eq!(format!("{}", vec_str.type_), "array<string>");

        let vec_i32 = mapper.map("Vec<i32>").expect("Vec<i32>");
        assert_eq!(format!("{}", vec_i32.type_), "array<integer>");
    }

    #[test]
    fn test_binary_legacy_migration() {
        let mapper = RustToJsonMapper;

        // Case 1: Vec<u8> should map to binary fields
        let res = mapper.map("Vec<u8>").expect("Failed to map");
        assert_eq!(res.type_, JsonType::String);
        assert_eq!(
            res.content_media_type.as_deref(),
            Some("application/octet-stream")
        );
        assert_eq!(res.content_encoding.as_deref(), Some("base64"));

        // Case 2: Option<Vec<u8>>
        let res_opt = mapper.map("Option<Vec<u8>>").expect("Failed to map");
        assert!(res_opt.nullable);
        assert_eq!(res_opt.type_, JsonType::String);
        assert_eq!(
            res_opt.content_media_type.as_deref(),
            Some("application/octet-stream")
        );

        // Case 3: Bytes
        let res_bytes = mapper.map("Bytes").expect("Failed to map");
        assert_eq!(res_bytes.type_, JsonType::String);
        assert_eq!(res_bytes.content_encoding.as_deref(), Some("base64"));
    }

    #[test]
    fn test_slices_binary() {
        let mapper = RustToJsonMapper;
        let res = mapper.map("&[u8]").expect("Failed to map");
        assert_eq!(res.type_, JsonType::String);
        assert_eq!(res.content_encoding.as_deref(), Some("base64"));

        let res_slice_u8 = mapper.map("[u8]").expect("Failed to map [u8]");
        assert_eq!(res_slice_u8.type_, JsonType::String);

        let res_slice_i32 = mapper.map("[i32]").expect("Failed to map [i32]");
        assert_eq!(format!("{}", res_slice_i32.type_), "array<integer>");

        let res_date = mapper.map("NaiveDate").expect("NaiveDate");
        assert_eq!(res_date.format.as_deref(), Some("date"));
        let res_dt = mapper.map("NaiveDateTime").expect("NaiveDateTime");
        assert_eq!(res_dt.format.as_deref(), Some("date-time"));
        let res_dec = mapper.map("Decimal").expect("Decimal");
        assert_eq!(res_dec.format.as_deref(), Some("decimal"));
        let res_box = mapper.map("Box<String>").expect("Box<String>");
        assert_eq!(res_box.type_, JsonType::String);
        let res_map = mapper.map("HashMap<String, i32>").expect("HashMap");
        assert_eq!(format!("{}", res_map.type_), "map<integer>");
        let res_bmap = mapper.map("BTreeMap<String, Uuid>").expect("BTreeMap");
        assert_eq!(format!("{}", res_bmap.type_), "map<string>");
        let res_custom = mapper.map("MyCustomModel").expect("MyCustomModel");
        assert_eq!(format!("{}", res_custom.type_), "$ref:MyCustomModel");
        assert_eq!(format!("{}", JsonType::String), "string");
        assert_eq!(format!("{}", JsonType::Number), "number");
        assert_eq!(format!("{}", JsonType::Boolean), "boolean");
        let (bin_schema, bin_opt) = map_rust_type_to_unwrapped_openapi_schema("Option<Vec<u8>>");
        assert!(bin_opt);
        assert_eq!(bin_schema["contentEncoding"], "base64");

        assert!(mapper.map("").is_err());
        assert!(mapper.map(";").is_err());
        assert!(mapper.map("&").is_err());
        assert!(mapper.map("[]").is_err());
        assert!(mapper.map("[(i32, i32)]").is_err());
        assert!(mapper.map("<i32>::").is_err());
        assert!(mapper.map("Option").is_err());
        assert!(mapper.map("Option<>").is_err());
        assert!(mapper.map("Option<'a>").is_err());
        assert!(mapper.map("Option<(i32, i32)>").is_err());
        assert!(mapper.map("Vec").is_err());
        assert!(mapper.map("Vec<'a>").is_err());
        assert!(mapper.map("Vec<(i32, i32)>").is_err());
        assert!(mapper.map("HashMap").is_err());
        assert!(mapper.map("HashMap<>").is_err());
        assert!(mapper.map("HashMap<String>").is_err());
        assert!(mapper.map("HashMap<String, 'a>").is_err());
        assert!(mapper.map("HashMap<String, (i32, i32)>").is_err());
        assert!(mapper.map("Secret").is_err());
        assert!(mapper.map("Secret<'a>").is_err());
        assert!(mapper.map("Secret<(i32, i32)>").is_err());
        assert!(mapper.map("(i32, String)").is_err());
    }
}
