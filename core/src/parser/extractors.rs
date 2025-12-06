//! # Extraction Logic
//!
//! High-level functions to parse Rust code into IR models.

use crate::error::{AppError, AppResult};
use crate::parser::attributes::extract_attributes;
use crate::parser::models::{ParsedEnum, ParsedField, ParsedModel, ParsedStruct, ParsedVariant};
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, SourceFile, SyntaxKind, SyntaxNode};

/// Extracts the names of all structs and enums defined in the provided Rust source code.
pub fn extract_struct_names(code: &str) -> AppResult<Vec<String>> {
    let parse = SourceFile::parse(code, Edition::Edition2021);
    let file = parse.tree();
    let mut names = Vec::new();

    for node in file.syntax().descendants() {
        if let Some(struct_def) = ast::Struct::cast(node.clone()) {
            if let Some(name) = struct_def.name() {
                names.push(name.text().to_string());
            }
        } else if let Some(enum_def) = ast::Enum::cast(node) {
            if let Some(name) = enum_def.name() {
                names.push(name.text().to_string());
            }
        }
    }

    Ok(names)
}

/// Parses a Rust source file to extract a struct definition.
/// Backward compatibility wrapper for `extract_model`.
pub fn extract_struct(code: &str, struct_name: &str) -> AppResult<ParsedStruct> {
    match extract_model(code, struct_name)? {
        ParsedModel::Struct(s) => Ok(s),
        ParsedModel::Enum(_) => Err(AppError::General(format!(
            "'{}' is an enum, expected struct",
            struct_name
        ))),
    }
}

/// Helper to extract only fields without full struct parsing if needed.
pub fn extract_struct_fields(code: &str, struct_name: &str) -> AppResult<Vec<ParsedField>> {
    let s = extract_struct(code, struct_name)?;
    Ok(s.fields)
}

/// Parsing function to extract a full model definition (struct or enum).
pub fn extract_model(code: &str, name: &str) -> AppResult<ParsedModel> {
    let parse = SourceFile::parse(code, Edition::Edition2021);
    let file = parse.tree();

    // 1. Try to find Struct
    if let Some(struct_def) = file.syntax().descendants().find_map(|node| {
        ast::Struct::cast(node).filter(|s| s.name().is_some_and(|n| n.text() == name))
    }) {
        return parse_struct_node(struct_def, name).map(ParsedModel::Struct);
    }

    // 2. Try to find Enum
    if let Some(enum_def) = file.syntax().descendants().find_map(|node| {
        ast::Enum::cast(node).filter(|e| e.name().is_some_and(|n| n.text() == name))
    }) {
        return parse_enum_node(enum_def, name).map(ParsedModel::Enum);
    }

    Err(AppError::General(format!("Model '{}' not found", name)))
}

fn parse_struct_node(struct_def: ast::Struct, name: &str) -> AppResult<ParsedStruct> {
    let struct_desc = extract_doc_comment(struct_def.syntax());
    let struct_attrs = extract_attributes(struct_def.syntax());

    let mut fields = Vec::new();

    if let Some(field_list) = struct_def.field_list() {
        match field_list {
            ast::FieldList::RecordFieldList(list) => {
                for field in list.fields() {
                    if let (Some(fname), Some(ty)) = (field.name(), field.ty()) {
                        let attrs = extract_attributes(field.syntax());
                        fields.push(ParsedField {
                            name: fname.text().to_string(),
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
        name: name.to_string(),
        description: struct_desc,
        rename: struct_attrs.rename,
        fields,
    })
}

fn parse_enum_node(enum_def: ast::Enum, name: &str) -> AppResult<ParsedEnum> {
    let desc = extract_doc_comment(enum_def.syntax());
    let attrs = extract_attributes(enum_def.syntax());

    let mut variants = Vec::new();

    if let Some(list) = enum_def.variant_list() {
        for variant in list.variants() {
            if let Some(vname) = variant.name() {
                let vattrs = extract_attributes(variant.syntax());
                let mut vty = None;

                // Handle single item tuple variants: Variant(Type)
                if let Some(fl) = variant.field_list() {
                    if let ast::FieldList::TupleFieldList(tfl) = fl {
                        // We strictly support polymorphism which wraps the subtype.
                        // So we look for exactly one field.
                        if let Some(first) = tfl.fields().next() {
                            if let Some(ty) = first.ty() {
                                vty = Some(ty.syntax().text().to_string());
                            }
                        }
                    }
                }

                variants.push(ParsedVariant {
                    name: vname.text().to_string(),
                    ty: vty,
                    description: extract_doc_comment(variant.syntax()),
                    rename: vattrs.rename,
                });
            }
        }
    }

    Ok(ParsedEnum {
        name: name.to_string(),
        description: desc,
        rename: attrs.rename,
        tag: attrs.tag,
        untagged: attrs.untagged,
        variants,
    })
}

/// Helper to extract `///` comments from a syntax node's trivia children.
pub(crate) fn extract_doc_comment(node: &SyntaxNode) -> Option<String> {
    let mut lines = Vec::new();

    for child in node.children_with_tokens() {
        if child.kind() == SyntaxKind::COMMENT {
            let text = child.to_string();
            if let Some(content) = text.strip_prefix("///") {
                lines.push(if let Some(stripped) = text.strip_prefix(' ') {
                    stripped.to_owned()
                } else {
                    content.to_owned()
                });
            }
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
    fn test_extract_enum_variants() {
        let code = r#"
            #[serde(tag = "type")]
            enum Pet {
                #[serde(rename = "cat")]
                Cat(CatStruct),
                Dog(DogStruct)
            }
        "#;
        let model = extract_model(code, "Pet").unwrap();
        if let ParsedModel::Enum(e) = model {
            assert_eq!(e.tag.as_deref(), Some("type"));
            assert_eq!(e.variants.len(), 2);
            assert_eq!(e.variants[0].name, "Cat");
            assert_eq!(e.variants[0].ty.as_deref(), Some("CatStruct"));
            assert_eq!(e.variants[0].rename.as_deref(), Some("cat"));
        } else {
            panic!("Expected enum");
        }
    }

    #[test]
    fn test_extract_enum_untagged() {
        let code = r#"
            #[serde(untagged)]
            enum Poly {
                A(i32),
                B(String)
            }
        "#;
        let model = extract_model(code, "Poly").unwrap();
        if let ParsedModel::Enum(e) = model {
            assert!(e.untagged);
            assert_eq!(e.variants[0].ty.as_deref(), Some("i32"));
        } else {
            panic!("Expected enum");
        }
    }
}
