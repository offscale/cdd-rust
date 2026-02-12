#![deny(missing_docs)]

//! # Code Generation
//!
//! Utilities for generating Rust source code from internal Intermediate Representations (IR).
//!
//! This module facilitates the transformation of `ParsedStruct` and `ParsedEnum` definitions
//! into valid, compilable Rust code. It handles:
//! - Dependency analysis (auto-injecting imports like `Uuid`, `chrono`, `serde`).
//! - Attribute injection (`derive`, `serde` options, `deprecated`).
//! - Formats `discriminator` mappings into Serde attributes (`tag`, `rename`, `alias`).
//! - Formatting and comments preservation including external docs links.

use crate::error::{AppError, AppResult};
use crate::parser::models::ParsedExternalDocs;
use crate::parser::{ParsedEnum, ParsedModel, ParsedStruct};
use ra_ap_edition::Edition;
use ra_ap_syntax::{ast, AstNode, SourceFile};
use std::collections::BTreeSet;

/// Creates a new AST `RecordField` node from strings.
pub fn make_record_field(
    name: &str,
    ty: &str,
    pub_vis: bool,
    indent_size: usize,
) -> AppResult<ast::RecordField> {
    let vis = if pub_vis { "pub " } else { "" };
    let indent = " ".repeat(indent_size);
    // Construct a dummy container to parse the field within a valid context
    let wrapper_code = format!("struct Wrapper {{\n{}{}{}: {},\n}}", indent, vis, name, ty);

    let parse = SourceFile::parse(&wrapper_code, Edition::Edition2021);

    if !parse.errors().is_empty() {
        let errs: Vec<String> = parse.errors().into_iter().map(|e| e.to_string()).collect();
        return Err(AppError::General(format!(
            "Failed to generate field node: {}",
            errs.join(", ")
        )));
    }

    let file = parse.tree();
    let struct_def = file
        .syntax()
        .descendants()
        .find_map(ast::Struct::cast)
        .ok_or_else(|| {
            AppError::General("Internal generation error: Wrapper struct not found".into())
        })?;

    let field_list = match struct_def.field_list() {
        Some(ast::FieldList::RecordFieldList(l)) => l,
        _ => {
            return Err(AppError::General(
                "Internal generation error: Wrapper field list mismatch".into(),
            ))
        }
    };

    field_list
        .fields()
        .next()
        .ok_or_else(|| AppError::General("Internal generation error: Field node not found".into()))
}

/// Generates a complete Rust source string for multiple Models (Structs or Enums).
///
/// This function aggregates all necessary imports for the set of models
/// and writes them sequentially into a single string.
pub fn generate_dtos(models: &[ParsedModel]) -> String {
    let mut code = String::new();
    let mut imports = BTreeSet::new();

    // 1. Analyze imports for all models
    imports.insert("use serde::{Deserialize, Serialize};".to_string());
    imports.insert("use utoipa::ToSchema;".to_string());

    for model in models {
        collect_imports(model, &mut imports);
    }

    // 2. Write Imports
    for import in imports {
        code.push_str(&import);
        code.push('\n');
    }
    code.push('\n');

    // 3. Write Definitions
    for (i, model) in models.iter().enumerate() {
        match model {
            ParsedModel::Struct(s) => code.push_str(&generate_dto_body(s)),
            ParsedModel::Enum(e) => code.push_str(&generate_enum_body(e)),
        }
        if i < models.len() - 1 {
            code.push('\n');
        }
    }

    code
}

/// Generates a Rust source string for a single struct, including imports.
pub fn generate_dto(dto: &ParsedStruct) -> String {
    generate_dtos(&[ParsedModel::Struct(dto.clone())])
}

/// Helper to generate documentation string with optional external docs link.
fn generate_doc_comment(
    description: Option<&String>,
    external: Option<&ParsedExternalDocs>,
    indent: &str,
) -> String {
    let mut code = String::new();
    if let Some(desc) = description {
        for line in desc.lines() {
            code.push_str(&format!("{}/// {}\n", indent, line));
        }
    }

    if let Some(ext) = external {
        if !code.is_empty() {
            code.push_str(&format!("{}///\n", indent));
        }
        let desc = ext.description.as_deref().unwrap_or("See also");
        code.push_str(&format!("{}/// {}: <{}>\n", indent, desc, ext.url));
    }
    code
}

/// Helper to generate the body of a single struct (without file-level imports).
fn generate_dto_body(dto: &ParsedStruct) -> String {
    let mut code = String::new();

    // Docs
    code.push_str(&generate_doc_comment(
        dto.description.as_ref(),
        dto.external_docs.as_ref(),
        "",
    ));

    // Deprecated
    if dto.is_deprecated {
        code.push_str("#[deprecated]\n");
    }

    // Derives
    code.push_str("#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]\n");

    // Struct Attributes (Rename)
    if let Some(rename) = &dto.rename {
        code.push_str(&format!("#[serde(rename = \"{}\")]\n", rename));
    }

    code.push_str(&format!("pub struct {} {{\n", dto.name));

    for field in &dto.fields {
        // Field Docs
        code.push_str(&generate_doc_comment(
            field.description.as_ref(),
            field.external_docs.as_ref(),
            "    ",
        ));

        // Field Deprecated
        if field.is_deprecated {
            code.push_str("    #[deprecated]\n");
        }

        // Field Attributes (Rename/Skip/Flatten)
        let mut attrs = Vec::new();
        if let Some(rename) = &field.rename {
            attrs.push(format!("rename = \"{}\"", rename));
        }
        if field.is_skipped {
            attrs.push("skip".to_string());
        }

        // Handle Map Flattening
        if field.name == "additional_properties" && field.ty.contains("HashMap") {
            attrs.push("flatten".to_string());
        }

        if !attrs.is_empty() {
            code.push_str(&format!("    #[serde({})]\n", attrs.join(", ")));
        }

        // Field Definition
        code.push_str(&format!("    pub {}: {},\n", field.name, field.ty));
    }

    code.push_str("}\n");
    code
}

/// Helper to generate the body of a single enum.
fn generate_enum_body(en: &ParsedEnum) -> String {
    let mut code = String::new();

    code.push_str(&generate_doc_comment(
        en.description.as_ref(),
        en.external_docs.as_ref(),
        "",
    ));

    if let Some(mapping) = &en.discriminator_mapping {
        if !mapping.is_empty() {
            code.push_str("///\n/// **Discriminator Mapping:**\n");
            for (key, val) in mapping {
                code.push_str(&format!("/// * `{}` -> `{}`\n", key, val));
            }
        }
    }
    if let Some(default_mapping) = &en.discriminator_default_mapping {
        if !default_mapping.is_empty() {
            code.push_str("///\n/// **Discriminator Default Mapping:** ");
            code.push_str(default_mapping);
            code.push('\n');
        }
    }

    if en.is_deprecated {
        code.push_str("#[deprecated]\n");
    }

    code.push_str("#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]\n");

    // Attributes
    let mut serde_attrs = Vec::new();
    if let Some(rename) = &en.rename {
        serde_attrs.push(format!("rename = \"{}\"", rename));
    }
    if let Some(tag) = &en.tag {
        serde_attrs.push(format!("tag = \"{}\"", tag));
    }
    if en.untagged {
        serde_attrs.push("untagged".to_string());
    }

    if !serde_attrs.is_empty() {
        code.push_str(&format!("#[serde({})]\n", serde_attrs.join(", ")));
    }

    code.push_str(&format!("pub enum {} {{\n", en.name));

    for variant in &en.variants {
        if let Some(desc) = &variant.description {
            for line in desc.lines() {
                code.push_str(&format!("    /// {}\n", line));
            }
        }

        if variant.is_deprecated {
            code.push_str("    #[deprecated]\n");
        }

        if let Some(r) = &variant.rename {
            code.push_str(&format!("    #[serde(rename = \"{}\")]\n", r));
        }

        if let Some(aliases) = &variant.aliases {
            for alias in aliases {
                code.push_str(&format!("    #[serde(alias = \"{}\")]\n", alias));
            }
        }

        if let Some(ty) = &variant.ty {
            code.push_str(&format!("    {}({}),\n", variant.name, ty));
        } else {
            code.push_str(&format!("    {},\n", variant.name));
        }
    }

    code.push_str("}\n");
    code
}

/// Analyzes a model's fields to determine required imports.
/// This handles flattened composition structs by checking all fields contained within.
fn collect_imports(model: &ParsedModel, imports: &mut BTreeSet<String>) {
    let types: Vec<&String> = match model {
        ParsedModel::Struct(s) => s.fields.iter().map(|f| &f.ty).collect(),
        ParsedModel::Enum(e) => e.variants.iter().filter_map(|v| v.ty.as_ref()).collect(),
    };

    for ty in types {
        if ty.contains("Uuid") {
            imports.insert("use uuid::Uuid;".to_string());
        }
        if ty.contains("DateTime") || ty.contains("NaiveDateTime") {
            imports.insert("use chrono::{DateTime, NaiveDateTime, Utc};".to_string());
        }
        if ty.contains("NaiveDate") && !ty.contains("NaiveDateTime") {
            imports.insert("use chrono::NaiveDate;".to_string());
        }
        if ty.contains("Value") || ty.contains("serde_json") {
            imports.insert("use serde_json::Value;".to_string());
        }
        if ty.contains("Decimal") {
            imports.insert("use rust_decimal::Decimal;".to_string());
        }
        if ty.contains("HashMap") {
            imports.insert("use std::collections::HashMap;".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ParsedExternalDocs, ParsedField, ParsedVariant};
    use std::collections::BTreeMap;

    fn field(name: &str, ty: &str) -> ParsedField {
        ParsedField {
            name: name.into(),
            ty: ty.into(),
            description: None,
            rename: None,
            is_skipped: false,
            is_deprecated: false,
            external_docs: None,
        }
    }

    #[test]
    fn test_generate_dto_simple() {
        let dto = ParsedStruct {
            name: "Simple".into(),
            description: Some("A simple struct".into()),
            rename: None,
            rename_all: None,
            fields: vec![field("id", "i32")],
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
        };

        let code = generate_dto(&dto);
        assert!(code.contains("struct Simple"));
        assert!(code.contains("/// A simple struct"));
        assert!(code.contains("#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]"));
    }

    #[test]
    fn test_generate_dto_with_metadata() {
        let docs = ParsedExternalDocs {
            url: "https://example.com".into(),
            description: Some("More info".into()),
        };

        let dto = ParsedStruct {
            name: "Oldie".into(),
            description: Some("Desc".into()),
            rename: None,
            rename_all: None,
            fields: vec![ParsedField {
                name: "f".into(),
                ty: "i32".into(),
                description: None,
                rename: None,
                is_skipped: false,
                is_deprecated: true,
                external_docs: None,
            }],
            is_deprecated: true,
            deny_unknown_fields: false,
            external_docs: Some(docs),
        };

        let code = generate_dto(&dto);
        assert!(code.contains("#[deprecated]"));
        assert!(code.contains("struct Oldie"));
        assert!(code.contains("/// Desc"));
        assert!(code.contains("/// More info: <https://example.com>"));
        // Check field deprecation
        assert!(code.contains("    #[deprecated]"));
        assert!(code.contains("    pub f: i32"));
    }

    #[test]
    fn test_generate_enum_tagged_with_alias() {
        let en = ParsedEnum {
            name: "Pet".into(),
            description: Some("Polymorphic pet".into()),
            rename: None,
            rename_all: None,
            tag: Some("type".into()),
            untagged: false,
            is_deprecated: false,
            external_docs: None,
            variants: vec![ParsedVariant {
                name: "Cat".into(),
                ty: Some("CatInfo".into()),
                description: None,
                rename: Some("cat".into()),
                aliases: Some(vec!["kitty".into()]),
                is_deprecated: false,
            }],
            discriminator_mapping: None,
            discriminator_default_mapping: None,
        };

        let code = generate_dtos(&[ParsedModel::Enum(en)]);
        assert!(code.contains("pub enum Pet"));
        assert!(code.contains("#[serde(tag = \"type\")]"));
        assert!(code.contains("    #[serde(rename = \"cat\")]"));
        assert!(code.contains("    #[serde(alias = \"kitty\")]"));
        assert!(code.contains("    Cat(CatInfo),"));
    }

    #[test]
    fn test_generate_enum_with_mapping_comments() {
        let mut mapping = BTreeMap::new();
        mapping.insert(
            "mapped_val".to_string(),
            "#/components/schemas/Mapped".to_string(),
        );

        let en = ParsedEnum {
            name: "MappedEnum".into(),
            description: None,
            rename: None,
            rename_all: None,
            tag: Some("kind".into()),
            untagged: false,
            is_deprecated: false,
            external_docs: None,
            variants: vec![],
            discriminator_mapping: Some(mapping),
            discriminator_default_mapping: Some("Other".to_string()),
        };

        let code = generate_dtos(&[ParsedModel::Enum(en)]);
        assert!(code.contains("/// **Discriminator Mapping:**"));
        assert!(code.contains("/// * `mapped_val` -> `#/components/schemas/Mapped`"));
        assert!(code.contains("/// **Discriminator Default Mapping:** Other"));
    }

    #[test]
    fn test_generate_enum_primitive_variants() {
        // Test untagged enum with primitives: [String, i32]
        let en = ParsedEnum {
            name: "IntOrString".into(),
            description: None,
            rename: None,
            rename_all: None,
            tag: None,
            untagged: true,
            is_deprecated: false,
            external_docs: None,
            variants: vec![
                ParsedVariant {
                    name: "String".into(),
                    ty: Some("String".into()),
                    description: None,
                    rename: None,
                    aliases: None,
                    is_deprecated: false,
                },
                ParsedVariant {
                    name: "Integer".into(),
                    ty: Some("i32".into()),
                    description: None,
                    rename: None,
                    aliases: None,
                    is_deprecated: false,
                },
            ],
            discriminator_mapping: None,
            discriminator_default_mapping: None,
        };

        let code = generate_dtos(&[ParsedModel::Enum(en)]);
        assert!(code.contains("pub enum IntOrString"));
        assert!(code.contains("#[serde(untagged)]"));
        assert!(code.contains("    String(String),"));
        assert!(code.contains("    Integer(i32),"));
    }

    #[test]
    fn test_flattened_imports() {
        // Simulating a struct that resulted from allOf flattening
        // It has a Uuid field (from Base) and a Value field (from Extension)
        let dto = ParsedStruct {
            name: "Merged".into(),
            description: None,
            rename: None,
            rename_all: None,
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
            fields: vec![field("id", "Uuid"), field("meta", "serde_json::Value")],
        };

        let code = generate_dto(&dto);
        assert!(code.contains("use uuid::Uuid;"));
        assert!(code.contains("use serde_json::Value;"));
        // Ensure struct body is valid
        assert!(code.contains("pub id: Uuid,"));
        assert!(code.contains("pub meta: serde_json::Value,"));
    }

    #[test]
    fn test_additional_properties_handling() {
        // Struct with HashMap field representing additional properties
        let dto = ParsedStruct {
            name: "Dict".into(),
            description: None,
            rename: None,
            rename_all: None,
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
            fields: vec![
                field("static_field", "String"),
                ParsedField {
                    name: "additional_properties".into(),
                    ty: "std::collections::HashMap<String, i32>".into(),
                    description: Some("Props".into()),
                    rename: None,
                    is_skipped: false,
                    is_deprecated: false,
                    external_docs: None,
                },
            ],
        };

        let code = generate_dto(&dto);
        // Should inject import
        assert!(code.contains("use std::collections::HashMap;"));
        // Should inject flatten attribute
        assert!(code.contains("#[serde(flatten)]"));
        assert!(code.contains("pub additional_properties: std::collections::HashMap<String, i32>"));
    }
}
