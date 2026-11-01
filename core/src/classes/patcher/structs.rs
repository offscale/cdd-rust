#![deny(missing_docs)]

//! Struct-level AST patching utilities (fields, types, derives, and attributes).

use crate::classes::patcher::common::{check_needs_comma, detect_indent, find_struct};
use crate::error::{AppError, AppResult};
use ra_ap_edition::Edition;
use ra_ap_syntax::{
    ast::{self, HasAttrs, HasName},
    AstNode, SourceFile, SyntaxKind,
};

/// Inserts a new field into an existing struct in the source code.
///
/// # Arguments
/// * `source` - Input `source` parameter.
/// * `struct_name` - Input `struct_name` parameter.
/// * `field_node` - Input `field_node` parameter.
///
/// # Returns
/// Resulting `AppResult<String>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
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

    if let Some(prev) = r_curly
        .prev_token()
        .filter(|t| t.kind() == SyntaxKind::WHITESPACE)
    {
        if needs_comma {
            insert_pos = prev.text_range().start().into();
        } else if prev.text().ends_with('\n') {
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
///
/// # Arguments
/// * `source` - Input `source` parameter.
/// * `struct_name` - Input `struct_name` parameter.
/// * `field_name` - Input `field_name` parameter.
/// * `new_type` - Input `new_type` parameter.
///
/// # Returns
/// Resulting `AppResult<String>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
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
///
/// # Arguments
/// * `source` - Input `source` parameter.
/// * `struct_name` - Input `struct_name` parameter.
/// * `derive_trait` - Input `derive_trait` parameter.
///
/// # Returns
/// Resulting `AppResult<String>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
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

        let r_paren = attr
            .as_simple_call()
            .and_then(|(_, tt)| tt.r_paren_token())
            .ok_or_else(|| AppError::General("Derive attribute has no closing delimiter".into()))?;

        let insert_pos: usize = r_paren.text_range().start().into();

        let needs_comma = r_paren.prev_token().is_none_or(|prev| {
            prev.kind() != SyntaxKind::COMMA && prev.kind() != SyntaxKind::L_PAREN
        });

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
        let indent_string = struct_def
            .syntax()
            .first_token()
            .and_then(|t| t.prev_token())
            .filter(|prev| prev.kind() == SyntaxKind::WHITESPACE)
            .map(|prev| {
                let text = prev.text();
                text.rsplit_once('\n')
                    .map(|(_, last)| last.to_string())
                    .unwrap_or_else(|| text.to_string())
            })
            .unwrap_or_default();

        let patch = format!("#[derive({})]\n{}", derive_trait, indent_string);
        new_source.insert_str(insert_pos, &patch);
    }

    Ok(new_source)
}

/// Adds an attribute line (e.g., `#[serde(...)]`) to a struct.
///
/// # Arguments
/// * `source` - Input `source` parameter.
/// * `struct_name` - Input `struct_name` parameter.
/// * `attribute` - Input `attribute` parameter.
///
/// # Returns
/// Resulting `AppResult<String>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classes::emit::make_record_field;

    // --- Tests for add_struct_field ---
    #[test]
    fn test_insert_into_empty_struct() {
        let code = "struct User {}";
        let field = make_record_field("id", "i32", true, 4).expect("Failed to make record field");
        let new_code = add_struct_field(code, "User", &field).expect("Failed to add struct field");
        assert!(new_code.contains("pub id: i32,"));

        // Needs comma with trailing whitespace
        let code_ws = "struct User { name: String }";
        let res_ws = add_struct_field(code_ws, "User", &field).expect("add with ws");
        assert!(res_ws.contains("name: String,\n    pub id: i32,"));

        // Needs comma without trailing whitespace
        let code_tight = "struct User {name: String}";
        let res_tight = add_struct_field(code_tight, "User", &field).expect("add tight");
        assert!(res_tight.contains("name: String,\n    pub id: i32,"));

        // Already has comma and trailing newline
        let code_nl = "struct User {\n    name: String,\n}";
        let res_nl = add_struct_field(code_nl, "User", &field).expect("add nl");
        assert!(res_nl.contains("    pub id: i32,\n}"));

        // Already has comma and space (not ending with \n)
        let code_sp = "struct User { name: String, }";
        let res_sp = add_struct_field(code_sp, "User", &field).expect("add sp");
        assert!(res_sp.contains("pub id: i32,"));

        // Error cases: missing struct, unit struct, tuple struct, missing '}'
        assert!(add_struct_field("struct Other {}", "User", &field).is_err());
        assert!(add_struct_field("struct User;", "User", &field).is_err());
        assert!(add_struct_field("struct User(i32);", "User", &field).is_err());
        assert!(add_struct_field("struct User { a: i32", "User", &field).is_err());
    }

    // --- Tests for add_derive ---
    #[test]
    fn test_add_derive_simple() {
        let code = "struct A;";
        let res = add_derive(code, "A", "Debug").expect("Failed to add derive");
        assert!(res.contains("#[derive(Debug)]"));

        // Duplicate derive trait is a no-op
        let dup = add_derive(&res, "A", "Debug").expect("dup derive");
        assert_eq!(dup, res);

        // Empty derive `#[derive()]` and trailing comma `#[derive(Clone,)]`
        let empty_d = add_derive("#[derive()]\nstruct A;", "A", "Debug").expect("empty derive");
        assert!(empty_d.contains("#[derive(Debug)]"));
        let comma_d =
            add_derive("#[derive(Clone,)]\nstruct A;", "A", "Debug").expect("comma derive");
        assert!(comma_d.contains("#[derive(Clone,Debug)]"));

        // Preceded by non-whitespace token (e.g. `mod m {struct A;}`)
        let tight_mod = add_derive("mod m {struct A;}", "A", "Debug").expect("tight mod");
        assert!(tight_mod.contains("#[derive(Debug)]"));

        // Error cases: missing struct, bare `#[derive]`, unclosed `#[derive(Clone`
        assert!(add_derive("struct Other;", "A", "Debug").is_err());
        assert!(add_derive("#[derive]\nstruct A;", "A", "Debug").is_err());
        assert!(add_derive("#[derive(Clone\nstruct A;", "A", "Debug").is_err());
    }

    #[test]
    fn test_add_derive_append() {
        let code = "#[derive(Clone)]\nstruct A;";
        let res = add_derive(code, "A", "Debug").expect("Failed to add derive");
        assert!(res.contains("#[derive(Clone, Debug)]"));
    }

    #[test]
    fn test_add_derive_indented() {
        let code = "    struct A;";
        let res = add_derive(code, "A", "Debug").expect("Failed to add derive");
        assert!(res.contains("    #[derive(Debug)]"));
        assert!(res.contains("\n    struct A"));

        let code_nl = "mod m {\n    struct A;\n}";
        let res_nl = add_derive(code_nl, "A", "Debug").expect("indented with nl");
        assert!(res_nl.contains("#[derive(Debug)]\n    struct A;"));
    }

    // --- Tests for modify_struct_field_type ---
    #[test]
    fn test_modify_type() {
        let code = "struct A { y: u32, x: i32 }";
        let res = modify_struct_field_type(code, "A", "x", "String")
            .expect("Failed to modify struct field type");
        assert!(res.contains("x: String"));

        assert!(modify_struct_field_type("struct Other { x: i32 }", "A", "x", "String").is_err());
        assert!(modify_struct_field_type("struct A;", "A", "x", "String").is_err());
        assert!(modify_struct_field_type("struct A { y: i32 }", "A", "x", "String").is_err());
        assert!(modify_struct_field_type("struct A { x: }", "A", "x", "String").is_err());
    }

    #[test]
    fn test_add_attribute_generic() {
        let code = "struct A;";
        let res =
            add_struct_attribute(code, "A", "#[foo]").expect("Failed to add struct attribute");
        assert!(res.contains("#[foo]\nstruct A"));

        // Duplicate attribute is a no-op
        let dup = add_struct_attribute(&res, "A", "#[foo]").expect("dup attr");
        assert_eq!(dup, res);

        assert!(add_struct_attribute("struct Other;", "A", "#[foo]").is_err());
    }
}
