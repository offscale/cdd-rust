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
