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
        let raw_text = doc.text();
        let stripped = raw_text.trim_start_matches("///").trim_start_matches("//!");
        let text = stripped.trim().to_string();
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

    #[test]
    fn test_docstring_roundtrip_across_models() {
        use crate::classes::emit::generate_dtos;
        use crate::classes::parse::{
            extract_model, ParsedEnum, ParsedField, ParsedModel, ParsedStruct, ParsedVariant,
        };

        let models = vec![
            ParsedModel::Struct(ParsedStruct {
                name: "UserDoc".to_string(),
                description: Some("User summary\nDetailed bio".to_string()),
                rename: None,
                rename_all: None,
                fields: vec![ParsedField {
                    name: "email".to_string(),
                    ty: "String".to_string(),
                    description: Some("Primary email address".to_string()),
                    rename: None,
                    is_skipped: false,
                    is_read_only: false,
                    is_write_only: false,
                    is_deprecated: false,
                    external_docs: None,
                }],
                is_deprecated: false,
                deny_unknown_fields: false,
                external_docs: None,
            }),
            ParsedModel::Enum(ParsedEnum {
                name: "RoleDoc".to_string(),
                description: Some("Account role".to_string()),
                rename: None,
                rename_all: None,
                tag: None,
                untagged: false,
                variants: vec![ParsedVariant {
                    name: "Admin".to_string(),
                    ty: None,
                    description: Some("Full administrator".to_string()),
                    rename: None,
                    aliases: None,
                    is_deprecated: false,
                }],
                is_deprecated: false,
                external_docs: None,
                discriminator_mapping: None,
                discriminator_default_mapping: None,
            }),
        ];

        let emitted = generate_dtos(&models);
        let s =
            crate::classes::parse::extract_struct(&emitted, "UserDoc").expect("extract UserDoc");
        assert_eq!(s.description.as_deref(), Some("User summary\nDetailed bio"));
        assert_eq!(
            s.fields[0].description.as_deref(),
            Some("Primary email address")
        );

        let parsed_enum = extract_model(&emitted, "RoleDoc").expect("extract RoleDoc");
        let parsed_struct = extract_model(&emitted, "UserDoc").expect("extract UserDoc");
        for item in [parsed_enum, parsed_struct] {
            if let ParsedModel::Enum(e) = item {
                assert_eq!(e.description.as_deref(), Some("Account role"));
                assert_eq!(
                    e.variants[0].description.as_deref(),
                    Some("Full administrator")
                );
            }
        }
    }
}
