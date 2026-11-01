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

use crate::classes::parse::models::ParsedExternalDocs;
use crate::classes::parse::{ParsedEnum, ParsedModel, ParsedStruct};
use crate::error::{AppError, AppResult};
use ra_ap_edition::Edition;
use ra_ap_syntax::{ast, AstNode, SourceFile};
use std::collections::BTreeSet;

/// Creates a new AST `RecordField` node from strings.
///
/// # Arguments
/// * `name` - Input `name` parameter.
/// * `ty` - Input `ty` parameter.
/// * `pub_vis` - Input `pub_vis` parameter.
/// * `indent_size` - Input `indent_size` parameter.
///
/// # Returns
/// Resulting `AppResult<ast::RecordField>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
pub fn make_record_field(
    name: &str,
    ty: &str,
    pub_vis: bool,
    indent_size: usize,
) -> AppResult<ast::RecordField> {
    let vis = if pub_vis { "pub " } else { "" };
    let indent = " ".repeat(indent_size);
    let raw_field = format!("{}{}{}: {}", indent, vis, name, ty);
    let code = format!("struct Wrapper {{\n{},\n}}", raw_field);

    let parse = SourceFile::parse(&code, Edition::Edition2021);
    if !parse.errors().is_empty() {
        let errs: Vec<String> = parse.errors().iter().map(|e| e.to_string()).collect();
        return Err(AppError::General(format!(
            "Invalid field syntax '{}': {}",
            raw_field,
            errs.join(", ")
        )));
    }

    let file = parse.tree();
    file.syntax()
        .descendants()
        .find_map(ast::RecordField::cast)
        .ok_or_else(|| AppError::AstMutationError {
            target: name.to_string(),
            reason: format!("Failed to construct AST RecordField from '{raw_field}'"),
        })
}

/// Generates a complete Rust source string for multiple Models (Structs or Enums).
///
/// This function aggregates all necessary imports for the set of models
/// and writes them sequentially into a single string.
///
/// # Arguments
/// * `models` - Input `models` parameter.
///
/// # Returns
/// Resulting `String` value.
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
///
/// # Arguments
/// * `dto` - Input `dto` parameter.
///
/// # Returns
/// Resulting `String` value.
pub fn generate_dto(dto: &ParsedStruct) -> String {
    generate_dtos(&[ParsedModel::Struct(dto.clone())])
}

/// Surgically updates an existing Rust model file with the provided `models`, using
/// [`crate::classes::diff::calculate_diff`] and [`crate::classes::patcher`] to patch
/// existing structs in-place and appending any newly introduced models.
///
/// # Arguments
/// * `existing_source` - Current content of the Rust models file (may be empty).
/// * `models` - Slice of target [`ParsedModel`] definitions.
///
/// # Returns
/// * `AppResult<String>` containing the updated, compilable Rust source code.
pub fn update_model_file(existing_source: &str, models: &[ParsedModel]) -> AppResult<String> {
    if existing_source.trim().is_empty() {
        return Ok(generate_dtos(models));
    }

    let mut current = existing_source.to_string();
    let mut imports = BTreeSet::new();
    imports.insert("use serde::{Deserialize, Serialize};".to_string());
    imports.insert("use utoipa::ToSchema;".to_string());
    for model in models {
        collect_imports(model, &mut imports);
    }
    for imp in imports {
        current = crate::classes::patcher::add_import(&current, &imp)?;
    }

    let existing_names = crate::classes::parse::extract_struct_names(&current)?;

    for model in models {
        match model {
            ParsedModel::Struct(s) if existing_names.contains(&s.name) => {
                current = crate::classes::patcher::add_derive(&current, &s.name, "Serialize")?;
                current = crate::classes::patcher::add_derive(&current, &s.name, "Deserialize")?;
                current = crate::classes::patcher::add_derive(&current, &s.name, "ToSchema")?;
                if let Some(rename) = &s.rename {
                    let attr = format!("#[serde(rename = \"{}\")]", rename);
                    current =
                        crate::classes::patcher::add_struct_attribute(&current, &s.name, &attr)?;
                }

                let diffs = crate::classes::diff::calculate_diff(&current, &s.name, s)?;
                for diff in diffs {
                    match diff {
                        crate::classes::diff::Diff::MissingField { name, ty } => {
                            let field_node = make_record_field(&name, &ty, true, 4)?;
                            current = crate::classes::patcher::add_struct_field(
                                &current,
                                &s.name,
                                &field_node,
                            )?;
                        }
                        crate::classes::diff::Diff::TypeMismatch {
                            field, expected, ..
                        } => {
                            current = crate::classes::patcher::modify_struct_field_type(
                                &current, &s.name, &field, &expected,
                            )?;
                        }
                        crate::classes::diff::Diff::Renamed { .. } => {}
                    }
                }
            }
            ParsedModel::Struct(s) => {
                current.push_str("\n\n");
                current.push_str(&generate_dto_body(s));
            }
            ParsedModel::Enum(e) if !existing_names.contains(&e.name) => {
                current.push_str("\n\n");
                current.push_str(&generate_enum_body(e));
            }
            ParsedModel::Enum(_) => {}
        }
    }

    Ok(current)
}

/// Helper to generate documentation string with optional external docs link.
///
/// # Arguments
/// * `description` - Input `description` parameter.
/// * `external` - Input `external` parameter.
/// * `indent` - Input `indent` parameter.
///
/// # Returns
/// Resulting `String` value.
fn generate_doc_comment(
    description: Option<&String>,
    external: Option<&ParsedExternalDocs>,
    indent: &str,
) -> String {
    let mut code = description
        .map(|desc| crate::docstrings::emit::format_outer_doc(desc, indent))
        .unwrap_or_default();

    if let Some(ext) = external {
        if !code.is_empty() {
            code.push_str(&format!("{}///\n", indent));
        }
        let desc = ext.description.as_deref().unwrap_or("See also");
        code.push_str(&crate::docstrings::emit::format_outer_doc(
            &format!("{}: <{}>", desc, ext.url),
            indent,
        ));
    }
    code
}

/// Helper to generate the body of a single struct (without file-level imports).
///
/// # Arguments
/// * `dto` - Input `dto` parameter.
///
/// # Returns
/// Resulting `String` value.
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
    code.push_str("#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]\n");
    code.push_str("#[serde(default)]\n");

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
        if !field.is_skipped {
            if field.is_read_only {
                attrs.push("skip_deserializing".to_string());
            }
            if field.is_write_only {
                attrs.push("skip_serializing".to_string());
            }
        }

        // Handle Map Flattening
        if field.name == "additional_properties" && field.ty.contains("HashMap") {
            attrs.push("flatten".to_string());
        }

        if !attrs.is_empty() {
            code.push_str(&format!("    #[serde({})]\n", attrs.join(", ")));
        }

        // Field Definition
        let mut escaped_name = field.name.clone();
        if crate::openapi::parse::models::RustIdent::is_keyword(&escaped_name) {
            escaped_name = format!("r#{}", escaped_name);
        }
        code.push_str(&format!("    pub {}: {},\n", escaped_name, field.ty));
    }

    code.push_str("}\n");
    code
}

/// Helper to generate the body of a single enum.
///
/// # Arguments
/// * `en` - Input `en` parameter.
///
/// # Returns
/// Resulting `String` value.
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
            code.push_str(&crate::docstrings::emit::format_outer_doc(desc, "    "));
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
///
/// # Arguments
/// * `model` - Input `model` parameter.
/// * `imports` - Input `imports` parameter.
fn collect_imports(model: &ParsedModel, imports: &mut BTreeSet<String>) {
    let types: Vec<&String> = match model {
        ParsedModel::Struct(s) => s.fields.iter().map(|f| &f.ty).collect(),
        ParsedModel::Enum(e) => e.variants.iter().filter_map(|v| v.ty.as_ref()).collect(),
    };

    for ty in types {
        if ty.contains("Uuid") {
            imports.insert("use uuid::Uuid;".to_string());
        }
        if ty.contains("DateTime") {
            imports.insert("use chrono::{DateTime, NaiveDateTime, Utc};".to_string());
        } else if ty.contains("NaiveDate") {
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
    use crate::classes::parse::{ParsedExternalDocs, ParsedField, ParsedVariant};
    use std::collections::BTreeMap;

    fn field(name: &str, ty: &str) -> ParsedField {
        ParsedField {
            name: name.into(),
            ty: ty.into(),
            description: None,
            rename: None,
            is_skipped: false,
            is_read_only: false,
            is_write_only: false,
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
        assert!(code.contains("#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Default)]"));
        assert!(code.contains("#[serde(default)]"));
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
                is_read_only: false,
                is_write_only: false,
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
    fn test_generate_dto_read_write_only_fields() {
        let dto = ParsedStruct {
            name: "Access".into(),
            description: None,
            rename: None,
            rename_all: None,
            fields: vec![
                ParsedField {
                    name: "id".into(),
                    ty: "Uuid".into(),
                    description: None,
                    rename: None,
                    is_skipped: false,
                    is_read_only: true,
                    is_write_only: false,
                    is_deprecated: false,
                    external_docs: None,
                },
                ParsedField {
                    name: "password".into(),
                    ty: "String".into(),
                    description: None,
                    rename: None,
                    is_skipped: false,
                    is_read_only: false,
                    is_write_only: true,
                    is_deprecated: false,
                    external_docs: None,
                },
            ],
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
        };

        let code = generate_dto(&dto);
        assert!(code.contains("pub struct Access"));
        assert!(code.contains("#[serde(skip_deserializing)]"));
        assert!(code.contains("#[serde(skip_serializing)]"));
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
                    is_read_only: false,
                    is_write_only: false,
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

        // Test make_record_field with pub_vis=false and error cases
        let priv_field = make_record_field("count", "usize", false, 2).expect("priv field");
        assert_eq!(priv_field.to_string(), "count: usize");
        assert!(make_record_field("bad name", "i32", true, 4).is_err());

        // Test generate_dtos with multiple models, struct/enum rename, skipped/keyword field, NaiveDate/DateTime/Decimal, and unit/deprecated/documented enum variants
        let s_full = ParsedStruct {
            name: "FullStruct".into(),
            description: None,
            rename: Some("CustomFull".into()),
            rename_all: None,
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: Some(ParsedExternalDocs {
                url: "https://docs.example.com".into(),
                description: None,
            }),
            fields: vec![
                ParsedField {
                    name: "type".into(),
                    ty: "NaiveDate".into(),
                    description: Some("Keyword field".into()),
                    rename: Some("kind".into()),
                    is_skipped: false,
                    is_read_only: false,
                    is_write_only: false,
                    is_deprecated: false,
                    external_docs: None,
                },
                ParsedField {
                    name: "secret".into(),
                    ty: "Decimal".into(),
                    description: None,
                    rename: None,
                    is_skipped: true,
                    is_read_only: false,
                    is_write_only: false,
                    is_deprecated: false,
                    external_docs: None,
                },
                ParsedField {
                    name: "additional_properties".into(),
                    ty: "String".into(),
                    description: None,
                    rename: None,
                    is_skipped: false,
                    is_read_only: false,
                    is_write_only: false,
                    is_deprecated: false,
                    external_docs: None,
                },
                field("created", "DateTime<Utc>"),
                field("updated", "NaiveDateTime"),
                field("raw", "Value"),
                field("num", "serde_json::Number"),
            ],
        };

        let e_full = ParsedEnum {
            name: "FullEnum".into(),
            description: None,
            rename: Some("CustomEnum".into()),
            rename_all: None,
            tag: None,
            untagged: false,
            is_deprecated: true,
            external_docs: None,
            variants: vec![ParsedVariant {
                name: "UnitVar".into(),
                ty: None,
                description: Some("Unit variant doc".into()),
                rename: None,
                aliases: None,
                is_deprecated: true,
            }],
            discriminator_mapping: Some(BTreeMap::new()),
            discriminator_default_mapping: Some(String::new()),
        };

        let e_plain = ParsedEnum {
            name: "PlainEnum".into(),
            description: None,
            rename: None,
            rename_all: None,
            tag: None,
            untagged: false,
            is_deprecated: false,
            external_docs: None,
            variants: vec![],
            discriminator_mapping: None,
            discriminator_default_mapping: None,
        };

        let multi_code = generate_dtos(&[
            ParsedModel::Struct(s_full),
            ParsedModel::Enum(e_full),
            ParsedModel::Enum(e_plain),
        ]);
        assert!(multi_code.contains("use chrono::NaiveDate;"));
        assert!(multi_code.contains("use chrono::{DateTime, NaiveDateTime, Utc};"));
        assert!(multi_code.contains("use rust_decimal::Decimal;"));
        assert!(multi_code.contains("/// See also: <https://docs.example.com>"));
        assert!(multi_code.contains("#[serde(rename = \"CustomFull\")]"));
        assert!(multi_code.contains("pub r#type: NaiveDate,"));
        assert!(multi_code.contains("#[serde(skip)]"));
        assert!(multi_code.contains("#[serde(rename = \"CustomEnum\")]"));
        assert!(multi_code.contains("/// Unit variant doc"));
        assert!(multi_code.contains("UnitVar,"));

        assert!(make_record_field("//", "", false, 0).is_err());
    }

    #[test]
    fn test_update_model_file_diffs_and_patches_existing_and_appends_new() {
        let target_user = ParsedStruct {
            name: "User".into(),
            description: Some("Updated user".into()),
            rename: Some("UserDto".into()),
            rename_all: None,
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
            fields: vec![
                field("id", "i64"),
                field("email", "String"),
                ParsedField {
                    name: "user_name".into(),
                    ty: "String".into(),
                    description: None,
                    rename: Some("userName".into()),
                    is_skipped: false,
                    is_read_only: false,
                    is_write_only: false,
                    is_deprecated: false,
                    external_docs: None,
                },
            ],
        };
        let new_tag = ParsedStruct {
            name: "Tag".into(),
            description: None,
            rename: None,
            rename_all: None,
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
            fields: vec![field("label", "String")],
        };
        let status_enum = ParsedEnum {
            name: "Status".into(),
            description: None,
            rename: None,
            rename_all: None,
            tag: None,
            untagged: false,
            is_deprecated: false,
            external_docs: None,
            variants: vec![ParsedVariant {
                name: "Active".into(),
                ty: None,
                description: None,
                rename: None,
                aliases: None,
                is_deprecated: false,
            }],
            discriminator_mapping: None,
            discriminator_default_mapping: None,
        };

        // 1. Empty source delegates to generate_dtos
        let from_empty =
            update_model_file("", &[ParsedModel::Struct(target_user.clone())]).expect("from empty");
        assert!(from_empty.contains("pub struct User"));

        // 2. Existing source with missing field (`email`), type mismatch (`id: i32` -> `i64`),
        // renamed field (`userName` -> `user_name`), existing enum, and new struct + enum
        let existing = "pub struct User {\n    pub id: i32,\n    pub userName: String,\n}\npub struct UnrenamedExisting {\n    pub label: String,\n}\npub enum ExistingEnum {\n    A,\n}";
        let unrenamed_existing = ParsedStruct {
            name: "UnrenamedExisting".into(),
            ..new_tag.clone()
        };
        let patched = update_model_file(
            existing,
            &[
                ParsedModel::Struct(target_user),
                ParsedModel::Struct(unrenamed_existing),
                ParsedModel::Struct(new_tag),
                ParsedModel::Enum(status_enum.clone()),
                ParsedModel::Enum(ParsedEnum {
                    name: "ExistingEnum".into(),
                    ..status_enum
                }),
            ],
        )
        .expect("patch existing");

        assert!(patched.contains("use serde::{Deserialize, Serialize};"));
        assert!(patched.contains("use utoipa::ToSchema;"));
        assert!(patched.contains("#[serde(rename = \"UserDto\")]"));
        assert!(patched.contains("pub id: i64"));
        assert!(patched.contains("pub email: String"));
        assert!(patched.contains("pub struct Tag"));
        assert!(patched.contains("pub enum Status"));
    }
}
