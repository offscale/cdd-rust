use crate::error::{AppError, AppResult};
use crate::patcher::common::{check_needs_comma, detect_indent, find_struct};
use ra_ap_edition::Edition;
use ra_ap_syntax::{
    ast::{self, HasAttrs, HasName},
    AstNode, SourceFile, SyntaxKind,
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
