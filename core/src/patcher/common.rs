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
