#![deny(missing_docs)]

//! # Attribute Operations
//!
//! Internal logic for parsing `#[serde(...)]` and `#[oai(...)]` attributes
//! directly from `ra_ap_syntax` token trees without regular expressions.

use crate::classes::parse::models::RenameRule;
use ra_ap_syntax::ast::{self};
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, SyntaxToken};

/// Helper struct for attributes extracted from a single AST node.
#[derive(Default, Debug)]
pub struct AttrInfo {
    /// The `rename` value if present.
    pub rename: Option<String>,
    /// The `rename_all` rule if present.
    pub rename_all: Option<RenameRule>,
    /// Whether the `skip` flag was found.
    pub is_skipped: bool,
    /// Whether the field is skipped during serialization (`skip` or `skip_serializing`).
    pub skip_serializing: bool,
    /// Whether the field is skipped during deserialization (`skip` or `skip_deserializing`).
    pub skip_deserializing: bool,
    /// The `tag` value (for enums) if present.
    pub tag: Option<String>,
    /// Whether the `untagged` flag was found (for enums).
    pub untagged: bool,
    /// Whether `deny_unknown_fields` was found.
    pub deny_unknown_fields: bool,
}

/// Analyzes attributes on a syntax node to find `serde` or `oai` configurations.
///
/// # Arguments
/// * `node` - Reference to the [`SyntaxNode`] whose direct `#[serde(...)]` / `#[oai(...)]` attributes are inspected.
///
/// # Returns
/// * Populated [`AttrInfo`] describing all discovered serialization/OpenAPI directives.
pub fn extract_attributes(node: &SyntaxNode) -> AttrInfo {
    let mut info = AttrInfo::default();

    for meta in node
        .children()
        .filter_map(ast::Attr::cast)
        .filter_map(|a| a.meta())
    {
        let is_target = meta
            .path()
            .is_some_and(|p| matches!(p.to_string().as_str(), "serde" | "oai"));
        if is_target {
            if let Some(tt) = meta.token_tree() {
                parse_attribute_token_tree(&tt, &mut info);
            }
        }
    }

    info
}

/// Parses the non-trivia tokens of a `#[serde(...)]` or `#[oai(...)]` [`ast::TokenTree`].
fn parse_attribute_token_tree(tt: &ast::TokenTree, info: &mut AttrInfo) {
    let tokens: Vec<SyntaxToken> = tt
        .syntax()
        .children_with_tokens()
        .filter_map(|el| el.into_token())
        .filter(|tok| !tok.kind().is_trivia())
        .collect();

    let mut idx = 0;
    while idx < tokens.len() {
        let tok = &tokens[idx];
        if tok.kind() == SyntaxKind::IDENT {
            match tok.text() {
                "skip" => {
                    info.is_skipped = true;
                    info.skip_serializing = true;
                    info.skip_deserializing = true;
                }
                "skip_serializing" => {
                    info.skip_serializing = true;
                }
                "skip_deserializing" => {
                    info.skip_deserializing = true;
                }
                "untagged" => {
                    info.untagged = true;
                }
                "deny_unknown_fields" => {
                    info.deny_unknown_fields = true;
                }
                key @ ("rename" | "rename_all" | "tag") => {
                    if let Some(val) = extract_assigned_string_literal(&tokens, idx) {
                        match key {
                            "rename" => info.rename = Some(val),
                            "rename_all" => info.rename_all = RenameRule::parse(&val),
                            _ => info.tag = Some(val),
                        }
                        idx += 2;
                    }
                }
                _ => {}
            }
        }
        idx += 1;
    }
}

/// Extracts the unquoted string literal from a `key = "value"` token sequence starting at `key_idx`.
fn extract_assigned_string_literal(tokens: &[SyntaxToken], key_idx: usize) -> Option<String> {
    let eq_tok = tokens.get(key_idx + 1)?;
    let val_tok = tokens.get(key_idx + 2)?;
    if eq_tok.kind() == SyntaxKind::EQ && val_tok.kind() == SyntaxKind::STRING {
        return Some(val_tok.text().trim_matches('"').to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra_ap_edition::Edition;
    use ra_ap_syntax::{ast, AstNode, SourceFile};

    fn parse_first_struct(code: &str) -> ast::Struct {
        let parse = SourceFile::parse(code, Edition::Edition2021);
        let file = parse.tree();
        file.syntax()
            .descendants()
            .find_map(ast::Struct::cast)
            .expect("struct missing")
    }

    fn parse_first_enum(code: &str) -> ast::Enum {
        let parse = SourceFile::parse(code, Edition::Edition2021);
        let file = parse.tree();
        file.syntax()
            .descendants()
            .find_map(ast::Enum::cast)
            .expect("enum missing")
    }

    #[test]
    fn test_extract_struct_rename() {
        let code = r#"
            #[serde(rename = "UserModel")]
            struct User {
                id: i32,
            }
        "#;
        let s = parse_first_struct(code);
        let info = extract_attributes(s.syntax());
        assert_eq!(info.rename.as_deref(), Some("UserModel"));
        assert!(!info.is_skipped);
        assert!(!info.skip_serializing);
        assert!(!info.skip_deserializing);
        assert!(info.tag.is_none());
        assert!(!info.untagged);
        assert!(info.rename_all.is_none());
        assert!(!info.deny_unknown_fields);
    }

    #[test]
    fn test_extract_enum_tag_and_untagged() {
        let code = r#"
            #[serde(tag = "kind", untagged)]
            enum Pet {
                Cat,
                Dog,
            }
        "#;
        let e = parse_first_enum(code);
        let info = extract_attributes(e.syntax());
        assert_eq!(info.tag.as_deref(), Some("kind"));
        assert!(info.untagged);
        assert!(!info.skip_serializing);
        assert!(!info.skip_deserializing);
        assert!(info.rename_all.is_none());
        assert!(!info.deny_unknown_fields);
    }

    #[test]
    fn test_extract_rename_all_and_deny_unknown() {
        let code = r#"
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct UserProfile {
                user_id: i32,
            }
        "#;
        let s = parse_first_struct(code);
        let info = extract_attributes(s.syntax());
        assert!(matches!(info.rename_all, Some(RenameRule::CamelCase)));
        assert!(info.deny_unknown_fields);
        assert!(!info.skip_serializing);
        assert!(!info.skip_deserializing);
    }

    #[test]
    fn test_extract_field_skip() {
        let code = r#"
            struct Secret {
                #[serde(skip)]
                token: String,
            }
        "#;
        let s = parse_first_struct(code);
        let field = s
            .syntax()
            .descendants()
            .find_map(ast::RecordField::cast)
            .expect("field missing");
        let info = extract_attributes(field.syntax());
        assert!(info.is_skipped);
        assert!(info.skip_serializing);
        assert!(info.skip_deserializing);
    }

    #[test]
    fn test_extract_field_skip_serializing() {
        let code = r#"
            struct Secret {
                #[serde(skip_serializing)]
                token: String,
            }
        "#;
        let s = parse_first_struct(code);
        let field = s
            .syntax()
            .descendants()
            .find_map(ast::RecordField::cast)
            .expect("field missing");
        let info = extract_attributes(field.syntax());
        assert!(!info.is_skipped);
        assert!(info.skip_serializing);
        assert!(!info.skip_deserializing);
    }

    #[test]
    fn test_extract_field_skip_deserializing() {
        let code = r#"
            struct Secret {
                #[serde(skip_deserializing)]
                token: String,
            }
        "#;
        let s = parse_first_struct(code);
        let field = s
            .syntax()
            .descendants()
            .find_map(ast::RecordField::cast)
            .expect("field missing");
        let info = extract_attributes(field.syntax());
        assert!(!info.is_skipped);
        assert!(!info.skip_serializing);
        assert!(info.skip_deserializing);
    }

    #[test]
    fn test_ignores_non_target_attributes_and_malformed_assignments() {
        let code = r#"
            #[derive(Debug)]
            #[serde]
            #[serde(unknown_flag, rename, tag = 123, rename = 'c')]
            #[]
            struct Ignored {
                #[doc = "not serde"]
                value: String,
            }
        "#;
        let s = parse_first_struct(code);
        let info = extract_attributes(s.syntax());
        assert!(info.rename.is_none());
        assert!(!info.is_skipped);
        assert!(info.tag.is_none());
        assert!(!info.untagged);

        let oai_code = r#"
            #[oai(rename = "OaiModel")]
            struct OaiUser {
                id: i32,
            }
        "#;
        let oai_s = parse_first_struct(oai_code);
        let oai_info = extract_attributes(oai_s.syntax());
        assert_eq!(oai_info.rename.as_deref(), Some("OaiModel"));
    }
}
