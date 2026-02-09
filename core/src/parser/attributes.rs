//! # Attribute Operations
//!
//! internal logic for parsing `#[serde(...)]` and `#[oai(...)]` attributes.

use ra_ap_syntax::ast::{self};
use ra_ap_syntax::{AstNode, SyntaxNode};
use regex::Regex;
use std::sync::OnceLock;

/// Helper struct for attributes extracted from a single node.
#[derive(Default, Debug)]
pub struct AttrInfo {
    /// The rename value if present.
    pub rename: Option<String>,
    /// Whether the skip flag was found.
    pub is_skipped: bool,
    /// The tag value (for enums) if present.
    pub tag: Option<String>,
    /// Whether the untagged flag was found (for enums).
    pub untagged: bool,
}

/// Analyzes attributes on a node to find `serde` or `oai` configurations.
pub fn extract_attributes(node: &SyntaxNode) -> AttrInfo {
    let mut info = AttrInfo::default();

    // Iterate over children that are attributes
    // Use generic casting iterator manually to work on SyntaxNode directly
    let attributes = node.children().filter_map(ast::Attr::cast);

    for attr in attributes {
        // We only care about "serde" or "oai" attributes for schema generation
        if let Some(meta) = attr.meta() {
            if let Some(path) = meta.path() {
                let ident = path.to_string();
                if ident == "serde" || ident == "oai" {
                    // Check the token tree content
                    if let Some(tt) = meta.token_tree() {
                        let content = tt.to_string();
                        parse_attribute_content(&content, &mut info);
                    }
                }
            }
        }
    }

    info
}

/// Parses the inner content of an attribute.
fn parse_attribute_content(content: &str, info: &mut AttrInfo) {
    static RENAME_RE: OnceLock<Regex> = OnceLock::new();
    let rename_re =
        RENAME_RE.get_or_init(|| Regex::new(r#"rename\s*=\s*"([^"]+)""#).expect("Invalid regex"));

    static SKIP_RE: OnceLock<Regex> = OnceLock::new();
    let skip_re = SKIP_RE.get_or_init(|| Regex::new(r#"\bskip\b"#).expect("Invalid regex"));

    static TAG_RE: OnceLock<Regex> = OnceLock::new();
    let tag_re =
        TAG_RE.get_or_init(|| Regex::new(r#"tag\s*=\s*"([^"]+)""#).expect("Invalid regex"));

    static UNTAGGED_RE: OnceLock<Regex> = OnceLock::new();
    let untagged_re =
        UNTAGGED_RE.get_or_init(|| Regex::new(r#"\buntagged\b"#).expect("Invalid regex"));

    if let Some(caps) = rename_re.captures(content) {
        if let Some(val) = caps.get(1) {
            info.rename = Some(val.as_str().to_string());
        }
    }

    if skip_re.is_match(content) {
        info.is_skipped = true;
    }

    if let Some(caps) = tag_re.captures(content) {
        if let Some(val) = caps.get(1) {
            info.tag = Some(val.as_str().to_string());
        }
    }

    if untagged_re.is_match(content) {
        info.untagged = true;
    }
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
        assert!(info.tag.is_none());
        assert!(!info.untagged);
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
            .field_list()
            .and_then(|list| match list {
                ast::FieldList::RecordFieldList(list) => list.fields().next(),
                _ => None,
            })
            .expect("field missing");
        let info = extract_attributes(field.syntax());
        assert!(info.is_skipped);
    }

    #[test]
    fn test_ignores_non_target_attributes() {
        let code = r#"
            #[derive(Debug)]
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
    }
}
