#![deny(missing_docs)]

//! # Code Patching
//!
//! Utilities for modifying Rust source code strings based on AST analysis.
//! Primarily used to insert new fields into existing structs/enums or modify
//! existing definitions safely without disrupting manual formatting or comments.

use crate::error::{AppError, AppResult};
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::HasModuleItem;
use ra_ap_syntax::{
    ast::{self, HasAttrs, HasName},
    AstNode, SourceFile, SyntaxKind, SyntaxToken,
};

/// Inserts a new field into an existing struct in the source code.
pub fn add_struct_field(
    source: &str,
    struct_name: &str,
    field_node: &ast::RecordField,
) -> AppResult<String> {
    let parse = SourceFile::parse(source, Edition::Edition2021);
    let file = parse.tree();

    let struct_def = find_struct(&file, struct_name)?;

    let field_list = match struct_def.field_list() {
        Some(ast::FieldList::RecordFieldList(l)) => l,
        None => {
            return Err(AppError::General(format!(
                "Struct '{}' is not a record struct (cannot add named fields)",
                struct_name
            )))
        }
        Some(_) => {
            return Err(AppError::General(format!(
                "Struct '{}' is not a record struct",
                struct_name
            )))
        }
    };

    let r_curly = field_list
        .r_curly_token()
        .ok_or_else(|| AppError::General("Invalid struct syntax: missing '}'".into()))?;

    let indent = detect_indent(&field_list).unwrap_or_else(|| "    ".to_string());

    let mut insert_pos: usize = r_curly.text_range().start().into();
    let mut prefix_newline = true;

    let needs_comma = check_needs_comma(&r_curly);

    if needs_comma {
        if let Some(prev) = r_curly.prev_token() {
            if prev.kind() == SyntaxKind::WHITESPACE {
                insert_pos = prev.text_range().start().into();
            }
        }
    } else if let Some(prev) = r_curly.prev_token() {
        if prev.kind() == SyntaxKind::WHITESPACE && prev.text().ends_with('\n') {
            prefix_newline = false;
        }
    }

    let mut patch = String::new();

    if needs_comma {
        patch.push(',');
    }

    if prefix_newline {
        patch.push('\n');
    }

    patch.push_str(&indent);
    patch.push_str(&field_node.to_string());
    patch.push(',');
    patch.push('\n');

    let mut new_source = source.to_string();
    new_source.insert_str(insert_pos, &patch);

    Ok(new_source)
}

/// Modifies the type of an existing field in a struct.
pub fn modify_struct_field_type(
    source: &str,
    struct_name: &str,
    field_name: &str,
    new_type: &str,
) -> AppResult<String> {
    let parse = SourceFile::parse(source, Edition::Edition2021);
    let file = parse.tree();

    let struct_def = find_struct(&file, struct_name)?;

    let field_list = match struct_def.field_list() {
        Some(ast::FieldList::RecordFieldList(l)) => l,
        _ => {
            return Err(AppError::General(format!(
                "Struct '{}' does not have named fields",
                struct_name
            )))
        }
    };

    let field = field_list
        .fields()
        .find(|f| f.name().is_some_and(|n| n.text() == field_name))
        .ok_or_else(|| {
            AppError::General(format!(
                "Field '{}' not found in struct '{}'",
                field_name, struct_name
            ))
        })?;

    let type_node = field.ty().ok_or_else(|| {
        AppError::General(format!("Field '{}' has no type definition", field_name))
    })?;

    let range = type_node.syntax().text_range();
    let start: usize = range.start().into();
    let end: usize = range.end().into();

    let mut new_source = source.to_string();
    new_source.replace_range(start..end, new_type);

    Ok(new_source)
}

/// Adds a trait to the derive attribute of a struct.
pub fn add_derive(source: &str, struct_name: &str, derive_trait: &str) -> AppResult<String> {
    let parse = SourceFile::parse(source, Edition::Edition2021);
    let file = parse.tree();
    let struct_def = find_struct(&file, struct_name)?;

    let derive_attr = struct_def
        .attrs()
        .find(|attr| attr.simple_name().as_deref() == Some("derive"));

    let mut new_source = source.to_string();

    if let Some(attr) = derive_attr {
        if attr.to_string().contains(derive_trait) {
            return Ok(new_source);
        }

        let tt = attr
            .token_tree()
            .ok_or_else(|| AppError::General("Derive attribute has no token tree".into()))?;

        let r_paren = tt.r_paren_token().ok_or_else(|| {
            AppError::General("Derive attribute missing closing parenthesis".into())
        })?;

        let insert_pos: usize = r_paren.text_range().start().into();

        let needs_comma = if let Some(prev) = r_paren.prev_token() {
            prev.kind() != SyntaxKind::COMMA && prev.kind() != SyntaxKind::L_PAREN
        } else {
            true
        };

        let patch = if needs_comma {
            format!(", {}", derive_trait)
        } else {
            derive_trait.to_string()
        };

        new_source.insert_str(insert_pos, &patch);
    } else {
        // Insert new derive
        let insert_pos: usize = struct_def.syntax().text_range().start().into();

        // Calculate indentation
        // We look directly at the token preceding the struct
        // We map to String to own the data and avoid lifetime issues with `prev`
        let indent_string = if let Some(token) = struct_def.syntax().first_token() {
            if let Some(prev) = token.prev_token() {
                if prev.kind() == SyntaxKind::WHITESPACE {
                    let text = prev.text();
                    if let Some((_, last)) = text.rsplit_once('\n') {
                        last.to_string()
                    } else {
                        // No newline, take partial text (e.g. start of file spacing)
                        text.to_string()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let patch = format!("#[derive({})]\n{}", derive_trait, indent_string);
        new_source.insert_str(insert_pos, &patch);
    }

    Ok(new_source)
}

/// Adds an import to the file if it does not already exist.
pub fn add_import(source: &str, import_statement: &str) -> AppResult<String> {
    let clean_stmt = import_statement.trim();
    let check_stmt = clean_stmt.trim_end_matches(';');

    if source.contains(check_stmt) {
        return Ok(source.into());
    }

    let parse = SourceFile::parse(source, Edition::Edition2021);
    let file = parse.tree();

    let mut last_use_node: Option<ra_ap_syntax::SyntaxNode> = None;
    for item in file.items() {
        if let ast::Item::Use(u) = item {
            last_use_node = Some(u.syntax().clone());
        }
    }

    let insert_pos;
    let patch;

    if let Some(node) = last_use_node {
        insert_pos = usize::from(node.text_range().end());
        patch = format!("\n{}", clean_stmt);
    } else {
        insert_pos = 0;
        patch = format!("{}\n", clean_stmt);
    }

    let mut new_source = source.to_string();
    new_source.insert_str(insert_pos, &patch);
    Ok(new_source)
}

/// Scans the entire file for structs and injects `#[derive(ToSchema)]` and the necessary import.
pub fn inject_openapi_attributes(source: &str) -> AppResult<String> {
    // 1. Add Import
    let mut current_source = add_import(source, "use utoipa::ToSchema;")?;

    // 2. Scan to find targets (names only)
    let struct_names = {
        let parse = SourceFile::parse(&current_source, Edition::Edition2021);
        let names: Vec<String> = parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Struct::cast)
            .filter_map(|s| s.name())
            .map(|n| n.text().to_string())
            .collect();
        names
    };

    // 3. Apply changes sequentially
    for name in struct_names {
        current_source = add_derive(&current_source, &name, "ToSchema")?;
    }

    Ok(current_source)
}

/// Adds an attribute line (e.g., `#[serde(...)]`) to a struct.
pub fn add_struct_attribute(source: &str, struct_name: &str, attribute: &str) -> AppResult<String> {
    let parse = SourceFile::parse(source, Edition::Edition2021);
    let file = parse.tree();
    let struct_def = find_struct(&file, struct_name)?;

    let struct_text = struct_def.syntax().text().to_string();
    if struct_text.contains(attribute) {
        return Ok(source.to_string());
    }

    let insert_pos: usize = struct_def.syntax().text_range().start().into();
    let mut new_source = source.to_string();

    new_source.insert_str(insert_pos, &format!("{}\n", attribute));

    Ok(new_source)
}

// --- Helpers ---

fn find_struct(file: &SourceFile, name: &str) -> AppResult<ast::Struct> {
    file.syntax()
        .descendants()
        .filter_map(ast::Struct::cast)
        .find(|s| s.name().is_some_and(|n| n.text() == name))
        .ok_or_else(|| AppError::General(format!("Struct '{}' not found in source file", name)))
}

fn detect_indent(list: &ast::RecordFieldList) -> Option<String> {
    let first_field = list.fields().next()?;
    let first_token = first_field.syntax().first_token()?;
    let token = first_token.prev_token()?;

    if token.kind() == SyntaxKind::WHITESPACE {
        let text = token.text();
        if let Some(pos) = text.rfind('\n') {
            return Some(text[pos + 1..].to_string());
        }
    }
    None
}

fn check_needs_comma(r_curly: &SyntaxToken) -> bool {
    let mut curr = r_curly.prev_token();
    while let Some(token) = curr {
        match token.kind() {
            SyntaxKind::WHITESPACE | SyntaxKind::COMMENT => {
                curr = token.prev_token();
            }
            SyntaxKind::L_CURLY | SyntaxKind::COMMA => {
                return false;
            }
            _ => {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::make_record_field;

    // --- Tests for add_struct_field ---
    #[test]
    fn test_insert_into_empty_struct() {
        let code = "struct User {}";
        let field = make_record_field("id", "i32", true, 4).unwrap();
        let new_code = add_struct_field(code, "User", &field).unwrap();
        assert!(new_code.contains("pub id: i32,"));
    }

    // --- Tests for add_derive ---
    #[test]
    fn test_add_derive_simple() {
        let code = "struct A;";
        let res = add_derive(code, "A", "Debug").unwrap();
        assert!(res.contains("#[derive(Debug)]"));
    }

    #[test]
    fn test_add_derive_append() {
        let code = "#[derive(Clone)]\nstruct A;";
        let res = add_derive(code, "A", "Debug").unwrap();
        assert!(res.contains("#[derive(Clone, Debug)]"));
    }

    #[test]
    fn test_add_derive_indented() {
        let code = "    struct A;";
        let res = add_derive(code, "A", "Debug").unwrap();
        assert!(res.contains("    #[derive(Debug)]"));
        assert!(res.contains("\n    struct A"));
    }

    // --- Tests for add_import ---
    #[test]
    fn test_add_import_new_file() {
        let code = "struct A;";
        let res = add_import(code, "use foo::Bar;").unwrap();
        assert!(res.starts_with("use foo::Bar;\n"));
    }

    #[test]
    fn test_add_import_existing() {
        let code = "use std::io;\nstruct A;";
        let res = add_import(code, "use foo::Bar;").unwrap();
        assert!(res.contains("use std::io;"));
        assert!(res.contains("use foo::Bar;"));
    }

    // --- Tests for inject_openapi_attributes ---
    #[test]
    fn test_inject_openapi_attributes_full_flow() {
        let code = r#"
            struct User {
                id: i32
            }

            struct Post {
                title: String
            }
        "#;

        let res = inject_openapi_attributes(code).unwrap();

        assert!(res.contains("use utoipa::ToSchema;"));
        assert!(res.contains("#[derive(ToSchema)]\n            struct User"));
        assert!(res.contains("#[derive(ToSchema)]\n            struct Post"));
    }

    #[test]
    fn test_inject_openapi_keeps_existing() {
        let code = r#"
            use std::collections::HashMap;

            #[derive(Debug)]
            struct User { id: i32 }
        "#;

        let res = inject_openapi_attributes(code).unwrap();

        assert!(res.contains("use std::collections::HashMap;"));
        assert!(res.contains("use utoipa::ToSchema;"));
        assert!(res.contains("#[derive(Debug, ToSchema)]"));
    }

    // --- Tests for modify_struct_field_type ---
    #[test]
    fn test_modify_type() {
        let code = "struct A { x: i32 }";
        let res = modify_struct_field_type(code, "A", "x", "String").unwrap();
        assert!(res.contains("x: String"));
    }

    #[test]
    fn test_add_attribute_generic() {
        let code = "struct A;";
        let res = add_struct_attribute(code, "A", "#[foo]").unwrap();
        assert!(res.contains("#[foo]\nstruct A"));
    }
}
