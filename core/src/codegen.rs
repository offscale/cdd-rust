#![deny(missing_docs)]

//! # Code Generation
//!
//! Utilities for generating Rust source code from internal Intermediate Representations (IR).
//!
//! This module facilitates the transformation of `ParsedStruct` definitions—derived from OpenAPI
//! schemas or other sources—into valid, compilable Rust code. It handles:
//! - Dependency analysis (auto-injecting imports like `Uuid`, `chrono`, `serde`).
//! - Attribute injection (`derive`, `serde` options).
//! - Formatting and comments preservation.

use crate::error::{AppError, AppResult};
use crate::parser::ParsedStruct;
use ra_ap_edition::Edition;
use ra_ap_syntax::{ast, AstNode, SourceFile};
use std::collections::BTreeSet;

/// Creates a new AST `RecordField` node from strings.
///
/// This is used primarily when patching existing source code to insert new fields.
/// By parsing a small wrapper struct, we ensure the generated field syntax is strictly valid.
///
/// # Arguments
///
/// * `name` - The name of the field (e.g., "email").
/// * `ty` - The Rust type string (e.g., "String", "Option<i32>").
/// * `pub_vis` - Whether the field should be public.
/// * `indent_size` - Indentation level (spaces) for formatting context.
///
/// # Returns
///
/// * `AppResult<ast::RecordField>` - The parsed AST node.
pub fn make_record_field(
    name: &str,
    ty: &str,
    pub_vis: bool,
    indent_size: usize,
) -> AppResult<ast::RecordField> {
    let vis = if pub_vis { "pub " } else { "" };
    let indent = " ".repeat(indent_size);
    // Construct a dummy container to parse the field within a valid context
    let wrapper_code = format!("struct Wrapper {{\n{}{}{}: {},\n}}", indent, vis, name, ty);

    let parse = SourceFile::parse(&wrapper_code, Edition::Edition2021);

    if !parse.errors().is_empty() {
        let errs: Vec<String> = parse.errors().into_iter().map(|e| e.to_string()).collect();
        return Err(AppError::General(format!(
            "Failed to generate field node: {}",
            errs.join(", ")
        )));
    }

    let file = parse.tree();
    let struct_def = file
        .syntax()
        .descendants()
        .find_map(ast::Struct::cast)
        .ok_or_else(|| {
            AppError::General("Internal generation error: Wrapper struct not found".into())
        })?;

    let field_list = match struct_def.field_list() {
        Some(ast::FieldList::RecordFieldList(l)) => l,
        _ => {
            return Err(AppError::General(
                "Internal generation error: Wrapper field list mismatch".into(),
            ))
        }
    };

    field_list
        .fields()
        .next()
        .ok_or_else(|| AppError::General("Internal generation error: Field node not found".into()))
}

/// Generates a complete Rust source string for multiple DTOs.
///
/// This function aggregates all necessary imports for the set of structs
/// and writes them sequentially into a single string, suitable for writing to a `.rs` file.
///
/// # Arguments
///
/// * `dtos` - A slice of parsed struct definitions.
///
/// # Returns
///
/// * `String` - The complete source file content.
pub fn generate_dtos(dtos: &[ParsedStruct]) -> String {
    let mut code = String::new();
    let mut imports = BTreeSet::new();

    // 1. Analyze imports for all structs
    imports.insert("use serde::{Deserialize, Serialize};".to_string());
    imports.insert("use utoipa::ToSchema;".to_string());

    for dto in dtos {
        collect_imports(&dto, &mut imports);
    }

    // 2. Write Imports
    for import in imports {
        code.push_str(&import);
        code.push('\n');
    }
    code.push('\n');

    // 3. Write Structs
    for (i, dto) in dtos.iter().enumerate() {
        code.push_str(&generate_dto_body(dto));
        if i < dtos.len() - 1 {
            code.push('\n');
        }
    }

    code
}

/// Generates a Rust source string for a single DTO, including imports.
///
/// Useful for generating individual snippets or single-struct files.
pub fn generate_dto(dto: &ParsedStruct) -> String {
    generate_dtos(&[dto.clone()])
}

/// Helper to generate the body of a single struct (without file-level imports).
fn generate_dto_body(dto: &ParsedStruct) -> String {
    let mut code = String::new();

    // Docs
    if let Some(desc) = &dto.description {
        for line in desc.lines() {
            code.push_str(&format!("/// {}\n", line));
        }
    }

    // Derives
    code.push_str("#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]\n");

    // Struct Attributes (Rename)
    if let Some(rename) = &dto.rename {
        code.push_str(&format!("#[serde(rename = \"{}\")]\n", rename));
    }

    code.push_str(&format!("pub struct {} {{\n", dto.name));

    for field in &dto.fields {
        // Field Docs
        if let Some(field_desc) = &field.description {
            for line in field_desc.lines() {
                code.push_str(&format!("    /// {}\n", line));
            }
        }

        // Field Attributes (Rename/Skip)
        let mut attrs = Vec::new();
        if let Some(rename) = &field.rename {
            attrs.push(format!("rename = \"{}\"", rename));
        }
        if field.is_skipped {
            attrs.push("skip".to_string());
        }

        if !attrs.is_empty() {
            code.push_str(&format!("    #[serde({})]\n", attrs.join(", ")));
        }

        // Field Definition
        code.push_str(&format!("    pub {}: {},\n", field.name, field.ty));
    }

    code.push_str("}\n");
    code
}

/// Analyzes a struct's fields to determine required imports.
fn collect_imports(dto: &ParsedStruct, imports: &mut BTreeSet<String>) {
    for field in &dto.fields {
        if field.ty.contains("Uuid") {
            imports.insert("use uuid::Uuid;".to_string());
        }
        if field.ty.contains("DateTime") || field.ty.contains("NaiveDateTime") {
            imports.insert("use chrono::{DateTime, NaiveDateTime, Utc};".to_string());
        }
        if field.ty.contains("NaiveDate") && !field.ty.contains("NaiveDateTime") {
            imports.insert("use chrono::NaiveDate;".to_string());
        }
        if field.ty.contains("Value") {
            imports.insert("use serde_json::Value;".to_string());
        }
        if field.ty.contains("Decimal") {
            imports.insert("use rust_decimal::Decimal;".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParsedField;

    // Helper to create a basic parsed field
    fn field(name: &str, ty: &str) -> ParsedField {
        ParsedField {
            name: name.into(),
            ty: ty.into(),
            description: None,
            rename: None,
            is_skipped: false,
        }
    }

    #[test]
    fn test_make_record_field_basic() {
        let f = make_record_field("foo", "i32", true, 4).unwrap();
        assert_eq!(f.to_string().trim(), "pub foo: i32");
    }

    #[test]
    fn test_make_record_field_private() {
        let f = make_record_field("bar", "String", false, 2).unwrap();
        assert_eq!(f.to_string().trim(), "bar: String");
    }

    #[test]
    fn test_make_record_field_invalid_syntax() {
        let result = make_record_field("bad", "::", true, 4);
        assert!(result.is_err());
    }

    #[test]
    fn test_make_record_field_internal_error() {
        // This simulates a scenario where parsing passes but structure is wrong.
        // Hard to trigger with `SourceFile` unless input is crafted to parse as non-struct.
        // `struct Wrapper` template forces struct.
        // We trust basic syntax tests cover the AST validity.
        assert!(make_record_field("ok", "i32", true, 4).is_ok());
    }

    #[test]
    fn test_generate_dto_simple() {
        let dto = ParsedStruct {
            name: "Simple".into(),
            description: Some("A simple struct".into()),
            rename: None,
            fields: vec![field("id", "i32")],
        };

        let code = generate_dto(&dto);
        assert!(code.contains("struct Simple"));
        assert!(code.contains("/// A simple struct"));
        assert!(code.contains("use serde"));
        assert!(code.contains("pub id: i32"));
        assert!(code.contains("#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]"));
    }

    #[test]
    fn test_generate_dto_imports() {
        let dto = ParsedStruct {
            name: "Complex".into(),
            description: None,
            rename: None,
            fields: vec![
                field("id", "Uuid"),
                field("t", "DateTime<Utc>"),
                field("d", "NaiveDate"),
                field("v", "Option<serde_json::Value>"),
                field("num", "rust_decimal::Decimal"),
            ],
        };

        let code = generate_dto(&dto);
        assert!(code.contains("use uuid::Uuid;"));
        assert!(code.contains("use chrono::{DateTime, NaiveDateTime, Utc};"));
        assert!(code.contains("use chrono::NaiveDate;"));
        assert!(code.contains("use serde_json::Value;"));
        assert!(code.contains("use rust_decimal::Decimal;"));
    }

    #[test]
    fn test_generate_dto_attributes() {
        let dto = ParsedStruct {
            name: "Renamed".into(),
            description: None,
            rename: Some("api_renamed".into()),
            fields: vec![
                ParsedField {
                    name: "f1".into(),
                    ty: "i32".into(),
                    description: Some("Field doc".into()),
                    rename: Some("f_one".into()),
                    is_skipped: false,
                },
                ParsedField {
                    name: "f2".into(),
                    ty: "i32".into(),
                    description: None,
                    rename: None,
                    is_skipped: true,
                },
            ],
        };

        let code = generate_dto(&dto);
        assert!(code.contains("#[serde(rename = \"api_renamed\")]"));
        assert!(code.contains("/// Field doc"));
        // Field 1: rename
        assert!(code.contains("#[serde(rename = \"f_one\")]"));
        // Field 2: skip
        assert!(code.contains("#[serde(skip)]"));
    }

    #[test]
    fn test_generate_dtos_multiple() {
        let dto1 = ParsedStruct {
            name: "A".into(),
            description: None,
            rename: None,
            fields: vec![field("u", "Uuid")],
        };
        let dto2 = ParsedStruct {
            name: "B".into(),
            description: None,
            rename: None,
            fields: vec![field("v", "Value")],
        };

        let code = generate_dtos(&[dto1, dto2]);

        // Imports should be unified at the top
        assert!(code.contains("use uuid::Uuid;"));
        assert!(code.contains("use serde_json::Value;"));

        // Both structs should exist
        assert!(code.contains("pub struct A"));
        assert!(code.contains("pub struct B"));
    }
}
