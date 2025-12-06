use crate::error::AppResult;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::HasModuleItem;
use ra_ap_syntax::{ast, AstNode, SourceFile};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
