use crate::error::{AppError, AppResult};
use ra_ap_syntax::ast::HasName;
use ra_ap_syntax::{ast, AstNode, SourceFile, SyntaxKind, SyntaxToken};

/// Finds a struct definition by name in the source file.
pub(crate) fn find_struct(file: &SourceFile, name: &str) -> AppResult<ast::Struct> {
    file.syntax()
        .descendants()
        .filter_map(ast::Struct::cast)
        .find(|s| s.name().is_some_and(|n| n.text() == name))
        .ok_or_else(|| AppError::General(format!("Struct '{}' not found in source file", name)))
}

/// Detects the indentation string used in a field list.
pub(crate) fn detect_indent(list: &ast::RecordFieldList) -> Option<String> {
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

/// Checks if a comma is needed before inserting a new field at the end of a list.
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

    fn parse_struct(code: &str) -> ast::Struct {
        let parse = SourceFile::parse(code, Edition::Edition2021);
        let file = parse.tree();
        file.syntax()
            .descendants()
            .find_map(ast::Struct::cast)
            .expect("struct missing")
    }

    #[test]
    fn test_find_struct_success_and_error() {
        let code = "struct User { id: i32 }";
        let parse = SourceFile::parse(code, Edition::Edition2021);
        let file = parse.tree();
        let found = find_struct(&file, "User").unwrap();
        assert_eq!(found.name().unwrap().text(), "User");

        let err = find_struct(&file, "Missing").unwrap_err();
        assert!(format!("{}", err).contains("Struct 'Missing' not found"));
    }

    #[test]
    fn test_detect_indent() {
        let code = "struct User {\n    id: i32,\n    name: String,\n}";
        let s = parse_struct(code);
        let list = s
            .field_list()
            .and_then(|list| match list {
                ast::FieldList::RecordFieldList(list) => Some(list),
                _ => None,
            })
            .expect("record field list missing");
        let indent = detect_indent(&list).expect("indent missing");
        assert_eq!(indent, "    ");
    }

    #[test]
    fn test_check_needs_comma() {
        let code_no_comma = "struct User { id: i32 }";
        let s = parse_struct(code_no_comma);
        let list = s
            .field_list()
            .and_then(|list| match list {
                ast::FieldList::RecordFieldList(list) => Some(list),
                _ => None,
            })
            .unwrap();
        let r_curly = list.syntax().last_token().unwrap();
        assert!(check_needs_comma(&r_curly));

        let code_with_comma = "struct User { id: i32, }";
        let s = parse_struct(code_with_comma);
        let list = s
            .field_list()
            .and_then(|list| match list {
                ast::FieldList::RecordFieldList(list) => Some(list),
                _ => None,
            })
            .unwrap();
        let r_curly = list.syntax().last_token().unwrap();
        assert!(!check_needs_comma(&r_curly));
    }
}
