#![deny(missing_docs)]

//! # Parser Module
//!
//! Handles parsing of Rust source code using the rust-analyzer syntax library.
//! Extracts structs, fields, documentation, and specific attributes (serde/oai).

use crate::error::{AppError, AppResult};
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, SourceFile, SyntaxKind, SyntaxNode};
use regex::Regex;
use std::sync::OnceLock;

/// Represents a field extracted from a struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedField {
    /// The name of the field.
    pub name: String,
    /// The raw Rust type string.
    pub ty: String,
    /// Extracted doc comments (if any).
    pub description: Option<String>,
    /// The name override for JSON/Schema (e.g. from `#[serde(rename="...")]`).
    pub rename: Option<String>,
    /// Whether the field is marked to be skipped in serialization/schema.
    pub is_skipped: bool,
}

/// Represents a fully parsed struct including field and doc metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedStruct {
    /// The struct name.
    pub name: String,
    /// The struct-level description from doc comments.
    pub description: Option<String>,
    /// The struct name override (e.g. `#[oai(rename="...")]`).
    pub rename: Option<String>,
    /// The list of fields.
    pub fields: Vec<ParsedField>,
}

/// Helper struct for attributes extracted from a single node.
#[derive(Default)]
struct AttrInfo {
    rename: Option<String>,
    is_skipped: bool,
}

/// Extracts the names of all structs defined in the provided Rust source code.
pub fn extract_struct_names(code: &str) -> AppResult<Vec<String>> {
    let parse = SourceFile::parse(code, Edition::Edition2021);
    let file = parse.tree();
    let mut names = Vec::new();

    for node in file.syntax().descendants() {
        if let Some(struct_def) = ast::Struct::cast(node) {
            if let Some(name) = struct_def.name() {
                names.push(name.text().to_string());
            }
        }
    }

    Ok(names)
}

/// Parsing function to extract a full struct definition including documentation and attributes.
///
/// Handles `#[serde(...)]` and `#[oai(...)]` attributes to detect renames or skips.
///
/// # Arguments
/// * `code` - Rust source code.
/// * `struct_name` - Name of struct to extract.
///
/// # Returns
/// * `ParsedStruct` containing fields, docs, and attribute metadata.
///
/// # Examples
/// ```
/// use cdd_core::parser::extract_struct;
///
/// let code = r#"
///     #[serde(rename = "MyUser")]
///     struct User {
///         #[serde(rename = "userId")]
///         id: i32,
///         #[serde(skip)]
///         hidden: String
///     }
/// "#;
/// let info = extract_struct(code, "User").unwrap();
/// assert_eq!(info.rename.as_deref(), Some("MyUser"));
/// assert_eq!(info.fields[0].rename.as_deref(), Some("userId"));
/// assert!(info.fields[1].is_skipped);
/// ```
pub fn extract_struct(code: &str, struct_name: &str) -> AppResult<ParsedStruct> {
    let parse = SourceFile::parse(code, Edition::Edition2021);
    let file = parse.tree();

    let struct_def = file
        .syntax()
        .descendants()
        .find_map(|node| {
            ast::Struct::cast(node).filter(|s| s.name().map_or(false, |n| n.text() == struct_name))
        })
        .ok_or_else(|| AppError::General(format!("Struct '{}' not found", struct_name)))?;

    // Extract struct level metadata
    let struct_desc = extract_doc_comment(struct_def.syntax());
    let struct_attrs = extract_attributes(struct_def.syntax());

    let mut fields = Vec::new();

    if let Some(field_list) = struct_def.field_list() {
        match field_list {
            ast::FieldList::RecordFieldList(list) => {
                for field in list.fields() {
                    if let (Some(name), Some(ty)) = (field.name(), field.ty()) {
                        let attrs = extract_attributes(field.syntax());

                        fields.push(ParsedField {
                            name: name.text().to_string(),
                            ty: ty.syntax().text().to_string(),
                            description: extract_doc_comment(field.syntax()),
                            rename: attrs.rename,
                            is_skipped: attrs.is_skipped,
                        });
                    }
                }
            }
            ast::FieldList::TupleFieldList(list) => {
                for (i, field) in list.fields().enumerate() {
                    if let Some(ty) = field.ty() {
                        let attrs = extract_attributes(field.syntax());

                        fields.push(ParsedField {
                            name: i.to_string(),
                            ty: ty.syntax().text().to_string(),
                            description: extract_doc_comment(field.syntax()),
                            rename: attrs.rename,
                            is_skipped: attrs.is_skipped,
                        });
                    }
                }
            }
        }
    }

    Ok(ParsedStruct {
        name: struct_name.to_string(),
        description: struct_desc,
        rename: struct_attrs.rename,
        fields,
    })
}

/// Helper to extract `///` comments from a syntax node's trivia children.
fn extract_doc_comment(node: &SyntaxNode) -> Option<String> {
    let mut lines = Vec::new();

    for child in node.children_with_tokens() {
        if child.kind() == SyntaxKind::COMMENT {
            let text = child.to_string();
            if text.starts_with("///") {
                let content = &text[3..];
                let content = if content.starts_with(' ') {
                    &content[1..]
                } else {
                    content
                };
                lines.push(content.to_string());
            }
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n").trim().to_string())
    }
}

/// Analyzes attributes on a node to find `serde` or `oai` configurations.
fn extract_attributes(node: &SyntaxNode) -> AttrInfo {
    let mut info = AttrInfo::default();

    // Iterate over children that are attributes
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

/// Parses the inner content of an attribute (e.g., `(rename = "foo", skip)`).
///
/// Uses regex to find keys safely.
fn parse_attribute_content(content: &str, info: &mut AttrInfo) {
    // Regex to find 'rename = "value"'
    // Matches: rename \s* = \s* "([^"]+)"
    static RENAME_RE: OnceLock<Regex> = OnceLock::new();
    let rename_re =
        RENAME_RE.get_or_init(|| Regex::new(r#"rename\s*=\s*"([^"]+)""#).expect("Invalid regex"));

    // Regex to find 'skip' word boundary
    static SKIP_RE: OnceLock<Regex> = OnceLock::new();
    let skip_re = SKIP_RE.get_or_init(|| Regex::new(r#"\bskip\b"#).expect("Invalid regex"));

    if let Some(caps) = rename_re.captures(content) {
        if let Some(val) = caps.get(1) {
            info.rename = Some(val.as_str().to_string());
        }
    }

    if skip_re.is_match(content) {
        info.is_skipped = true;
    }
}

/// Helper for Item 2.2 compatibility (extract only fields without full struct parsing if needed).
/// Now wraps generic `extract_struct`.
pub fn extract_struct_fields(code: &str, struct_name: &str) -> AppResult<Vec<ParsedField>> {
    let s = extract_struct(code, struct_name)?;
    Ok(s.fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serde_rename_field() {
        let code = r#"
            struct User {
                #[serde(rename = "userId")]
                id: i32
            }
        "#;
        let s = extract_struct(code, "User").unwrap();
        assert_eq!(s.fields[0].name, "id");
        assert_eq!(s.fields[0].rename.as_deref(), Some("userId"));
        assert!(!s.fields[0].is_skipped);
    }

    #[test]
    fn test_serde_skip_field() {
        let code = r#"
            struct Secret {
                #[serde(skip)]
                token: String
            }
        "#;
        let s = extract_struct(code, "Secret").unwrap();
        assert_eq!(s.fields[0].name, "token");
        assert!(s.fields[0].is_skipped);
    }

    #[test]
    fn test_oai_mixed_attributes() {
        let code = r#"
            #[oai(rename = "OpenApiStruct")]
            struct Test {
                #[oai(skip)]
                internal: i32,
                #[serde(rename = "vis_name")]
                visible: String
            }
        "#;
        let s = extract_struct(code, "Test").unwrap();

        // Struct rename
        assert_eq!(s.rename.as_deref(), Some("OpenApiStruct"));

        // Field 1 skip
        assert!(s.fields[0].is_skipped);

        // Field 2 rename
        assert_eq!(s.fields[1].rename.as_deref(), Some("vis_name"));
    }

    #[test]
    fn test_complex_formatting() {
        // Test with spaces and newlines in attributes
        let code = r#"
            struct Complex {
                #[serde(
                    rename = "weird"
                )]
                f: i32
            }
        "#;
        let s = extract_struct(code, "Complex").unwrap();
        assert_eq!(s.fields[0].rename.as_deref(), Some("weird"));
    }

    #[test]
    fn test_multiple_attributes() {
        // Ensure we handle multiple attribute blocks
        let code = r#"
            struct Multi {
                #[deprecated]
                #[serde(rename = "a")]
                f: i32
            }
        "#;
        let s = extract_struct(code, "Multi").unwrap();
        assert_eq!(s.fields[0].rename.as_deref(), Some("a"));
    }

    #[test]
    fn test_extract_struct_fields_backwards_compat() {
        // 2.2 functionality check
        let code = "struct A { x: i32 }";
        let fields = extract_struct_fields(code, "A").unwrap();
        assert_eq!(fields[0].name, "x");
        assert_eq!(fields[0].ty, "i32");
    }
}
