#![deny(missing_docs)]

//! Shared AST lookup and indentation helpers for source patching.

use crate::error::{AppError, AppResult};
use ra_ap_syntax::ast::HasName;
use ra_ap_syntax::{ast, AstNode, SourceFile, SyntaxKind, SyntaxToken};

/// Finds a struct definition by name in the source file.
///
/// # Arguments
/// * `file` - Input `file` parameter.
/// * `name` - Input `name` parameter.
///
/// # Returns
/// Resulting `AppResult<ast::Struct>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
pub(crate) fn find_struct(file: &SourceFile, name: &str) -> AppResult<ast::Struct> {
    file.syntax()
        .descendants()
        .filter_map(ast::Struct::cast)
        .find(|s| s.name().is_some_and(|n| n.text() == name))
        .ok_or_else(|| AppError::General(format!("Struct '{}' not found in source file", name)))
}

/// Detects the indentation string used in a field list.
///
/// # Arguments
/// * `list` - Input `list` parameter.
///
/// # Returns
/// Resulting `Option<String>` value.
pub(crate) fn detect_indent(list: &ast::RecordFieldList) -> Option<String> {
    let first_field = list.fields().next()?;
    let token = first_field.syntax().first_token()?.prev_token()?;

    if token.kind() == SyntaxKind::WHITESPACE {
        let text = token.text();
        if let Some(pos) = text.rfind('\n') {
            return Some(text[pos + 1..].to_string());
        }
    }
    None
}

/// Checks if a comma is needed before inserting a new field at the end of a list.
///
/// # Arguments
/// * `r_curly` - Input `r_curly` parameter.
///
/// # Returns
/// Resulting `bool` value.
pub(crate) fn check_needs_comma(r_curly: &SyntaxToken) -> bool {
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
    use ra_ap_edition::Edition;
    use ra_ap_syntax::{ast, AstNode, SourceFile};

    fn parse_record_list(code: &str) -> ast::RecordFieldList {
        let parse = SourceFile::parse(code, Edition::Edition2021);
        let file = parse.tree();
        file.syntax()
            .descendants()
            .find_map(ast::RecordFieldList::cast)
            .expect("record field list missing")
    }

    #[test]
    fn test_find_struct_success_and_error() {
        let code = "struct User { id: i32 }";
        let parse = SourceFile::parse(code, Edition::Edition2021);
        let file = parse.tree();
        let found = find_struct(&file, "User").expect("Failed to find struct");
        assert_eq!(found.name().expect("Failed to name").text(), "User");

        let err = find_struct(&file, "Missing").expect_err("expected error");
        assert!(format!("{}", err).contains("Struct 'Missing' not found"));
    }

    #[test]
    fn test_detect_indent() {
        let list = parse_record_list("struct User {\n    id: i32,\n    name: String,\n}");
        let indent = detect_indent(&list).expect("indent missing");
        assert_eq!(indent, "    ");

        // Single-line with space (no newline)
        let list_inline = parse_record_list("struct User { id: i32 }");
        assert!(detect_indent(&list_inline).is_none());

        // Single-line without space (prev_token is L_CURLY, not WHITESPACE)
        let list_tight = parse_record_list("struct User {id: i32}");
        assert!(detect_indent(&list_tight).is_none());

        // Empty struct (no fields)
        let list_empty = parse_record_list("struct User {}");
        assert!(detect_indent(&list_empty).is_none());
    }

    #[test]
    fn test_check_needs_comma() {
        let list = parse_record_list("struct User { id: i32 /* comment */ }");
        let r_curly = list.syntax().last_token().expect("Failed to last token");
        assert!(check_needs_comma(&r_curly));

        let list = parse_record_list("struct User { id: i32, }");
        let r_curly = list.syntax().last_token().expect("Failed to last token");
        assert!(!check_needs_comma(&r_curly));

        // Token at start of file has no prev_token -> loop ends and returns false
        let root = list.syntax().ancestors().last().expect("root");
        let first_tok = root.first_token().expect("first token");
        assert!(!check_needs_comma(&first_tok));
    }

    #[test]
    fn test_repeated_cst_parse_drops_cleanly() {
        for idx in 0..64 {
            let code = format!("struct Item{idx} {{\n    pub id: u64,\n}}");
            let parse = SourceFile::parse(&code, Edition::Edition2021);
            let file = parse.tree();
            let target = format!("Item{idx}");
            let found = find_struct(&file, &target).expect("struct must exist");
            assert_eq!(found.name().expect("struct name").text(), target);
        }
    }
}
