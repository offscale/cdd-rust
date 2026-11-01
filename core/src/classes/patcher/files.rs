#![deny(missing_docs)]

//! File-level AST patching utilities (such as import injection).

use crate::error::AppResult;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::HasModuleItem;
use ra_ap_syntax::{ast, AstNode, SourceFile, SyntaxKind};

/// Adds an import to the file if it does not already exist.
///
/// Inspects the `ra_ap_syntax` CST for existing `use` items so comments or string literals
/// containing the import text do not falsely suppress insertion. When no `use` items exist,
/// inserts the new `use` item after any leading inner attributes (`#![...]`) and module-level
/// doc comments (`//!`).
///
/// # Arguments
/// * `source` - Existing Rust source code.
/// * `import_statement` - The `use ...;` statement to inject.
///
/// # Returns
/// * `AppResult<String>` containing the updated source code.
pub fn add_import(source: &str, import_statement: &str) -> AppResult<String> {
    let clean_stmt = import_statement.trim();
    let check_stmt = clean_stmt.trim_end_matches(';').trim();

    let parse = SourceFile::parse(source, Edition::Edition2021);
    let file = parse.tree();

    let mut last_use_node: Option<ra_ap_syntax::SyntaxNode> = None;
    for item in file.items() {
        if let ast::Item::Use(u) = item {
            let existing = u.syntax().text().to_string();
            if existing.trim().trim_end_matches(';').trim() == check_stmt {
                return Ok(source.into());
            }
            last_use_node = Some(u.syntax().clone());
        }
    }

    let (insert_pos, patch) = if let Some(node) = last_use_node {
        (
            usize::from(node.text_range().end()),
            format!("\n{}", clean_stmt),
        )
    } else if let Some(header_end) = find_module_header_end(&file) {
        (header_end, format!("\n{}\n", clean_stmt))
    } else {
        (0, format!("{}\n", clean_stmt))
    };

    let mut new_source = source.to_string();
    new_source.insert_str(insert_pos, &patch);
    Ok(new_source)
}

/// Finds the byte offset after any leading inner attributes (`#![...]`) and module doc comments (`//!`).
///
/// # Arguments
/// * `file` - Input `file` parameter.
///
/// # Returns
/// Resulting `Option<usize>` value.
fn find_module_header_end(file: &SourceFile) -> Option<usize> {
    let mut end_pos: Option<usize> = None;

    for elem in file.syntax().children_with_tokens() {
        match elem.kind() {
            SyntaxKind::ATTR => {
                end_pos = Some(usize::from(elem.text_range().end()));
            }
            SyntaxKind::DOC_COMMENT if elem.to_string().starts_with("//!") => {
                end_pos = Some(usize::from(elem.text_range().end()));
            }
            SyntaxKind::WHITESPACE => {}
            _ => break,
        }
    }

    end_pos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_import_new_file() {
        let code = "struct A;";
        let res = add_import(code, "use foo::Bar;").expect("Failed to add import");
        assert!(res.starts_with("use foo::Bar;\n"));
    }

    #[test]
    fn test_add_import_existing() {
        let code = "use std::io;\nstruct A;";
        let res = add_import(code, "use foo::Bar;").expect("Failed to add import");
        assert!(res.contains("use std::io;"));
        assert!(res.contains("use foo::Bar;"));

        let duplicate = add_import(&res, "use foo::Bar;").expect("duplicate import");
        assert_eq!(duplicate, res);
    }

    #[test]
    fn test_add_import_after_inner_attrs_and_module_docs_and_ignores_comments() {
        let code = "#![deny(missing_docs)]\n//! Module docs mentioning use foo::Bar;\n// Regular comment use foo::Bar;\npub struct A;\n";
        let res = add_import(code, "use foo::Bar;").expect("must insert after inner header");
        let parsed = SourceFile::parse(&res, Edition::Edition2021);
        assert!(
            parsed.errors().is_empty(),
            "Expected valid syntax after import insertion, got errors: {:?}\nCode:\n{}",
            parsed.errors(),
            res
        );
        assert!(res.starts_with(
            "#![deny(missing_docs)]\n//! Module docs mentioning use foo::Bar;\nuse foo::Bar;\n"
        ));

        let comment_only = "// Leading regular comment in file without items\n";
        let res_comment = add_import(comment_only, "use foo::Bar;").expect("comment only");
        assert!(res_comment.starts_with("use foo::Bar;\n"));

        let outer_doc_only = "/// Leading outer doc comment without items\n";
        let res_outer_doc = add_import(outer_doc_only, "use foo::Bar;").expect("outer doc only");
        assert!(res_outer_doc.starts_with("use foo::Bar;\n"));
    }
}
