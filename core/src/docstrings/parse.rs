#![deny(missing_docs)]

//! # Docstring Parsing
//!
//! Extracts structured documentation text and tags from Rust AST nodes and comment strings.

use ra_ap_syntax::{ast::HasDocComments, AstToken};

/// Extracts combined documentation (`///` and `#[doc = "..."]`) from an AST node that supports doc comments.
///
/// # Arguments
/// * `node` - The syntax node implementing `HasDocComments`.
///
/// # Returns
/// * `Option<String>` containing the joined, trimmed documentation lines, or `None` if no doc comments exist.
pub fn extract_doc_comment<N: HasDocComments>(node: &N) -> Option<String> {
    let mut docs = Vec::new();
    for doc in node.doc_comments() {
        let text = doc
            .text()
            .trim_start_matches("///")
            .trim_start_matches("//!")
            .trim()
            .to_string();
        if !text.is_empty() {
            docs.push(text);
        }
    }
    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

/// Parses raw Rust source lines containing `///` or `//!` comments into a normalized docstring.
///
/// # Arguments
/// * `raw` - Raw comment block or source snippet.
///
/// # Returns
/// * `Option<String>` with normalized documentation text, or `None` if no doc lines are present.
pub fn parse_raw_doc_lines(raw: &str) -> Option<String> {
    let mut lines = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        let rest_opt = trimmed
            .strip_prefix("///")
            .or_else(|| trimmed.strip_prefix("//!"));
        if let Some(rest) = rest_opt {
            lines.push(rest.trim().to_string());
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n").trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra_ap_edition::Edition;
    use ra_ap_syntax::{ast, AstNode, SourceFile};

    #[test]
    fn test_extract_doc_comment_present_and_absent() {
        let code = r#"
        /// First line.
        ///
        /// Second line.
        pub struct DocumentedStruct;

        pub struct UndocumentedStruct;
        "#;
        let parsed = SourceFile::parse(code, Edition::Edition2021);
        let file = parsed.tree();
        let mut structs = file.syntax().descendants().filter_map(ast::Struct::cast);

        let first = structs.next().expect("expected first struct");
        assert_eq!(
            extract_doc_comment(&first),
            Some(
                "First line.
Second line."
                    .to_string()
            )
        );

        let second = structs.next().expect("expected second struct");
        assert_eq!(extract_doc_comment(&second), None);
    }

    #[test]
    fn test_parse_raw_doc_lines() {
        let raw = "/// Hello
//! World
// Ignored normal comment
";
        assert_eq!(
            parse_raw_doc_lines(raw),
            Some(
                "Hello
World"
                    .to_string()
            )
        );
        assert_eq!(parse_raw_doc_lines("// Regular comment only"), None);
    }
}
