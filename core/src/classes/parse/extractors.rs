#![deny(missing_docs)]

//! # Extraction Logic
//!
//! High-level functions to parse Rust code into IR models.

use crate::classes::parse::attributes::extract_attributes;
use crate::classes::parse::models::{
    ParsedEnum, ParsedField, ParsedModel, ParsedStruct, ParsedVariant,
};
use crate::docstrings::parse::extract_doc_comment;
use crate::error::{AppError, AppResult};
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, SourceFile};

/// Extracts the names of all structs and enums defined in the provided Rust source code.
pub fn extract_struct_names(code: &str) -> AppResult<Vec<String>> {
    let parse = SourceFile::parse(code, Edition::Edition2021);
    let file = parse.tree();
    let mut names = Vec::new();

    for node in file.syntax().descendants() {
        if let Some(name) = ast::Struct::cast(node.clone()).and_then(|s| s.name()) {
            names.push(name.text().to_string());
        } else if let Some(name) = ast::Enum::cast(node).and_then(|e| e.name()) {
            names.push(name.text().to_string());
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
    let struct_desc = extract_doc_comment(&struct_def);
    let struct_attrs = extract_attributes(struct_def.syntax());

    let mut fields = Vec::new();

    if let Some(field_list) = struct_def.field_list() {
        match field_list {
            ast::FieldList::RecordFieldList(list) => {
                for field in list.fields() {
                    let fname = field
                        .name()
                        .map(|n| n.text().to_string())
                        .unwrap_or_default();
                    let ty_str = field
                        .ty()
                        .map(|t| t.syntax().text().to_string())
                        .unwrap_or_default();
                    let attrs = extract_attributes(field.syntax());
                    let is_read_only =
                        !attrs.is_skipped && attrs.skip_deserializing && !attrs.skip_serializing;
                    let is_write_only =
                        !attrs.is_skipped && attrs.skip_serializing && !attrs.skip_deserializing;
                    fields.push(ParsedField {
                        name: fname,
                        ty: ty_str,
                        description: extract_doc_comment(&field),
                        rename: attrs.rename,
                        is_skipped: attrs.is_skipped,
                        is_read_only,
                        is_write_only,
                        is_deprecated: false, // Not parsed from AST yet
                        external_docs: None,
                    });
                }
            }
            ast::FieldList::TupleFieldList(list) => {
                for (i, field) in list.fields().enumerate() {
                    let ty_str = field
                        .ty()
                        .map(|t| (t as ra_ap_syntax::ast::Type).syntax().text().to_string())
                        .unwrap_or_default();
                    let attrs = extract_attributes(field.syntax());
                    let is_read_only =
                        !attrs.is_skipped && attrs.skip_deserializing && !attrs.skip_serializing;
                    let is_write_only =
                        !attrs.is_skipped && attrs.skip_serializing && !attrs.skip_deserializing;
                    fields.push(ParsedField {
                        name: i.to_string(),
                        ty: ty_str,
                        description: extract_doc_comment(&field),
                        rename: attrs.rename,
                        is_skipped: attrs.is_skipped,
                        is_read_only,
                        is_write_only,
                        is_deprecated: false,
                        external_docs: None,
                    });
                }
            }
        }
    }

    Ok(ParsedStruct {
        name: name.to_string(),
        description: struct_desc,
        rename: struct_attrs.rename,
        rename_all: struct_attrs.rename_all,
        fields,
        is_deprecated: false, // Not parsed from AST yet
        deny_unknown_fields: struct_attrs.deny_unknown_fields,
        external_docs: None,
    })
}

fn parse_enum_node(enum_def: ast::Enum, name: &str) -> AppResult<ParsedEnum> {
    let desc = extract_doc_comment(&enum_def);
    let attrs = extract_attributes(enum_def.syntax());

    let mut variants = Vec::new();

    for variant in enum_def
        .variant_list()
        .into_iter()
        .flat_map(|l| l.variants())
    {
        let vname = variant
            .name()
            .map(|n| n.text().to_string())
            .unwrap_or_default();
        let vattrs = extract_attributes(variant.syntax());
        let vty = match variant.field_list() {
            Some(ast::FieldList::TupleFieldList(tfl)) => tfl
                .fields()
                .next()
                .and_then(|f| f.ty())
                .map(|ty| ty.syntax().text().to_string()),
            _ => None,
        };

        variants.push(ParsedVariant {
            name: vname,
            ty: vty,
            description: extract_doc_comment(&variant),
            rename: vattrs.rename,
            aliases: None,
            is_deprecated: false,
        });
    }

    Ok(ParsedEnum {
        name: name.to_string(),
        description: desc,
        rename: attrs.rename,
        rename_all: attrs.rename_all,
        tag: attrs.tag,
        untagged: attrs.untagged,
        variants,
        is_deprecated: false,
        external_docs: None,
        discriminator_mapping: None,
        discriminator_default_mapping: None,
    })
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
        let s = extract_struct(code, "User").expect("Failed to extract struct");
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
        let s = extract_struct(code, "Secret").expect("Failed to extract struct");
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
        let model = extract_model(code, "Pet").expect("Failed to extract model");
        assert_eq!(model.name(), "Pet");
        for m in [
            model,
            ParsedModel::Struct(extract_struct("struct S;", "S").expect("s")),
        ] {
            if let ParsedModel::Enum(e) = m {
                assert_eq!(e.tag.as_deref(), Some("type"));
                assert_eq!(e.variants.len(), 2);
                assert_eq!(e.variants[0].name, "Cat");
                assert_eq!(e.variants[0].ty.as_deref(), Some("CatStruct"));
                assert_eq!(e.variants[0].rename.as_deref(), Some("cat"));
            }
        }
    }

    #[test]
    fn test_extract_enum_untagged() {
        let code = r#"
            ///EnumDoc
            // regular comment
            #[serde(untagged)]
            enum Poly {
                A(i32),
                B(String),
                Unit,
            }

            struct UnitStruct;

            struct TupleStruct(
                #[serde(skip)] i32,
                #[serde(skip_deserializing)] String,
                #[serde(skip_serializing)] bool,
                #[serde(skip_serializing, skip_deserializing)] u64,
            );

            struct AccessRecord {
                #[serde(skip_deserializing)]
                ro: i32,
                #[serde(skip_serializing)]
                wo: i32,
                #[serde(skip_serializing, skip_deserializing)]
                both: i32,
            }
        "#;
        let names = extract_struct_names(code).expect("names");
        assert_eq!(
            names,
            vec!["Poly", "UnitStruct", "TupleStruct", "AccessRecord"]
        );

        let model = extract_model(code, "Poly").expect("Failed to extract model");
        assert_eq!(model.name(), "Poly");
        assert!(extract_struct(code, "Poly").is_err());
        assert!(extract_struct_fields(code, "Poly").is_err());

        let unit = extract_struct(code, "UnitStruct").expect("unit struct");
        assert!(unit.fields.is_empty());

        let tuple = extract_struct(code, "TupleStruct").expect("tuple struct");
        assert_eq!(tuple.fields.len(), 4);
        assert!(tuple.fields[0].is_skipped);
        assert!(tuple.fields[1].is_read_only);
        assert!(tuple.fields[2].is_write_only);
        assert!(!tuple.fields[3].is_read_only);
        assert!(!tuple.fields[3].is_write_only);

        let rec = extract_struct(code, "AccessRecord").expect("access record");
        assert!(rec.fields[0].is_read_only);
        assert!(rec.fields[1].is_write_only);
        assert!(!rec.fields[2].is_read_only);
        assert!(!rec.fields[2].is_write_only);
    }
}
