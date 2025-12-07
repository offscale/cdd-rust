#![deny(missing_docs)]

//! # Type Mapping
//!
//! Converts Rust types into a JSON Schema compatible representation.
//! Handles primitives, collections (Vec), and nullability (Option).
//!
//! Updated for OpenAPI 3.2 compliance regarding Binary Data (`contentMediaType`).

use crate::error::{AppError, AppResult};
// Import HasGenericArgs to access .generic_arg_list() on PathSegments
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasGenericArgs};
use ra_ap_syntax::{AstNode, SourceFile};
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
            JsonType::Ref(s) => write!(f, "$ref:{}", s),
        }
    }
}

/// Represents the schema definition for a mapped type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonSchema {
    /// The primary JSON type.
    pub type_: JsonType,
    /// Optional format specifier (e.g., "uuid", "date-time").
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
    fn map(&self, rust_type: &str) -> AppResult<JsonSchema>;
}

/// A standard implementation of `TypeMapper`.
pub struct RustToJsonMapper;

impl TypeMapper for RustToJsonMapper {
    fn map(&self, rust_type: &str) -> AppResult<JsonSchema> {
        // Wrap implementation to parse valid Rust syntax using a type alias
        let code = format!("type _Wrapper = {};", rust_type);
        let parse = SourceFile::parse(&code, Edition::Edition2021);
        let file = parse.tree();

        // Find the type alias definition
        let type_alias = file
            .syntax()
            .descendants()
            .find_map(ast::TypeAlias::cast)
            .ok_or_else(|| {
                AppError::General(format!("Failed to parse type string: {}", rust_type))
            })?;

        // Extract the type node
        let root_type = type_alias
            .ty()
            .ok_or_else(|| AppError::General(format!("Invalid type syntax: {}", rust_type)))?;

        // Recursively map the AST node
        map_ast_type(&root_type)
    }
}

/// Recursively maps an AST Type node to JsonSchema.
fn map_ast_type(ty: &ast::Type) -> AppResult<JsonSchema> {
    match ty {
        ast::Type::PathType(path_type) => {
            let path = path_type
                .path()
                .ok_or_else(|| AppError::General("Empty path".into()))?;
            let segment = path
                .segment()
                .ok_or_else(|| AppError::General("Empty segment".into()))?;
            let name_ref = segment
                .name_ref()
                .ok_or_else(|| AppError::General("No type name".into()))?;
            let name = name_ref.text();

            match name.as_str() {
                // Primitives
                "String" | "str" | "char" => Ok(simple(JsonType::String)),
                "bool" => Ok(simple(JsonType::Boolean)),
                "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
                | "u128" | "usize" => Ok(simple(JsonType::Integer)),
                "f32" | "f64" => Ok(simple(JsonType::Number)),

                // Complex / Formats
                "Uuid" => Ok(formatted(JsonType::String, "uuid")),
                "NaiveDateTime" | "DateTime" => Ok(formatted(JsonType::String, "date-time")),
                "NaiveDate" => Ok(formatted(JsonType::String, "date")),

                // Binary types (explicit Bytes wrapper)
                // Vec<u8> is handled via container generic check below, but bytes::Bytes is direct.
                "Bytes" | "ByteBuf" => Ok(binary_schema()),

                // Containers
                "Option" => handle_generic_wrapper(segment, true, |inner| inner),
                "Vec" => handle_generic_wrapper(segment, false, |inner| {
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

                // Fallback: User defined structs or unknown types become References
                other => Ok(simple(JsonType::Ref(other.to_string()))),
            }
        }
        // Correct AST variant for reference types (e.g. &str, &[u8])
        ast::Type::RefType(ref_type) => {
            let inner = ref_type
                .ty()
                .ok_or_else(|| AppError::General("Invalid reference".into()))?;

            // Check for slice &[u8]
            if let ast::Type::SliceType(ref slice) = inner {
                if let Some(slice_inner) = slice.ty() {
                    if slice_inner.to_string().trim() == "u8" {
                        return Ok(binary_schema());
                    }
                }
            }

            map_ast_type(&inner)
        }
        ast::Type::SliceType(slice) => {
            if let Some(inner) = slice.ty() {
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
            } else {
                Err(AppError::General("Invalid slice type".into()))
            }
        }
        _ => Err(AppError::General(format!(
            "Unsupported type structure: {:?}",
            ty
        ))),
    }
}

/// Helper to handle types like `Option<T>` or `Vec<T>`.
fn handle_generic_wrapper<F>(
    segment: ast::PathSegment,
    make_nullable: bool,
    wrap: F,
) -> AppResult<JsonSchema>
where
    F: FnOnce(JsonSchema) -> JsonSchema,
{
    // Need HasGenericArgs trait for this method
    let generic_args = segment
        .generic_arg_list()
        .ok_or_else(|| AppError::General("Missing generic arguments for container type".into()))?;

    // We assume the first argument is the type T.
    let first_arg = generic_args
        .generic_args()
        .next()
        .ok_or_else(|| AppError::General("Generic list empty".into()))?;

    match first_arg {
        ast::GenericArg::TypeArg(type_arg) => {
            let inner_ty = type_arg
                .ty()
                .ok_or_else(|| AppError::General("Invalid generic type".into()))?;
            let mut schema = map_ast_type(&inner_ty)?;

            if make_nullable {
                schema.nullable = true;
            }

            let result = wrap(schema);
            Ok(result)
        }
        _ => Err(AppError::General(
            "Unsupported generic argument type".into(),
        )),
    }
}

// Helpers for cleaner construction
fn simple(t: JsonType) -> JsonSchema {
    JsonSchema {
        type_: t,
        format: None,
        nullable: false,
        content_media_type: None,
        content_encoding: None,
    }
}

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

/// Heuristic to confirm if a type node dealing with integers is actually u8.
/// Used inside Vec processing because the recursive map returns generic "Integer".
fn is_u8_heuristic(ty: &ast::Type) -> bool {
    // This is a rough check by looking at the text.
    // map_ast_type is recursive, so when we are inside Vec<T>, 'ty' is 'Vec<u8>'.
    // We want to check T.
    let text = ty.to_string();
    text.contains("u8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_mapping() {
        let mapper = RustToJsonMapper;

        let cases = vec![
            ("i32", JsonType::Integer),
            ("f32", JsonType::Number),
            ("bool", JsonType::Boolean),
            ("String", JsonType::String),
        ];

        for (input, expected) in cases {
            let res = mapper.map(input).expect(input);
            assert_eq!(res.type_, expected);
        }
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
        let res = mapper.map("Option<i32>").unwrap();
        assert!(res.nullable);
        assert_eq!(res.type_, JsonType::Integer);
    }

    #[test]
    fn test_nested_complex() {
        let mapper = RustToJsonMapper;
        // Vec<Option<Uuid>> is weird but structurally parseable: array<integer(uuid) nullable>
        // Option<Vec<Uuid>> -> nullable array<string(uuid)>
        let res = mapper.map("Option<Vec<Uuid>>").unwrap();

        assert!(res.nullable);
        if let JsonType::Array(inner) = res.type_ {
            assert_eq!(inner.type_, JsonType::String);
            assert_eq!(inner.format.as_deref(), Some("uuid"));
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_binary_legacy_migration() {
        let mapper = RustToJsonMapper;

        // Case 1: Vec<u8> should map to binary fields
        let res = mapper.map("Vec<u8>").unwrap();
        assert_eq!(res.type_, JsonType::String);
        assert_eq!(
            res.content_media_type.as_deref(),
            Some("application/octet-stream")
        );
        assert_eq!(res.content_encoding.as_deref(), Some("base64"));

        // Case 2: Option<Vec<u8>>
        let res_opt = mapper.map("Option<Vec<u8>>").unwrap();
        assert!(res_opt.nullable);
        assert_eq!(res_opt.type_, JsonType::String);
        assert_eq!(
            res_opt.content_media_type.as_deref(),
            Some("application/octet-stream")
        );

        // Case 3: Bytes
        let res_bytes = mapper.map("Bytes").unwrap();
        assert_eq!(res_bytes.type_, JsonType::String);
        assert_eq!(res_bytes.content_encoding.as_deref(), Some("base64"));
    }

    #[test]
    fn test_slices_binary() {
        let mapper = RustToJsonMapper;
        let res = mapper.map("&[u8]").unwrap();
        assert_eq!(res.type_, JsonType::String);
        assert_eq!(res.content_encoding.as_deref(), Some("base64"));
    }
}
