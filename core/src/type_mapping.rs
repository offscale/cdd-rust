#![deny(missing_docs)]

//! # Type Mapping
//!
//! Converts Rust types into a JSON Schema compatible representation.
//! Handles primitives, collections (Vec), and nullability (Option).

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

                // Containers
                "Option" => handle_generic_wrapper(segment, true, |inner| inner),
                "Vec" => handle_generic_wrapper(segment, false, |inner| JsonSchema {
                    type_: JsonType::Array(Box::new(inner)),
                    format: None,
                    nullable: false,
                }),

                // Fallback: User defined structs or unknown types become References
                other => Ok(simple(JsonType::Ref(other.to_string()))),
            }
        }
        // Correct AST variant for reference types (e.g. &str)
        ast::Type::RefType(ref_type) => {
            let inner = ref_type
                .ty()
                .ok_or_else(|| AppError::General("Invalid reference".into()))?;
            map_ast_type(&inner)
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
    }
}

fn formatted(t: JsonType, fmt: &str) -> JsonSchema {
    JsonSchema {
        type_: t,
        format: Some(fmt.to_string()),
        nullable: false,
    }
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
}
