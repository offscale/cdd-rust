#![deny(missing_docs)]

//! # Diff Calculation
//!
//! Compares existing Rust source code against a desired target schema (represented as `ParsedStruct`).
//! Detects missing fields, type mismatches, and renamed fields (via attribute stability).

use crate::error::AppResult;
use crate::parser::{extract_struct, ParsedField, ParsedStruct};
use std::collections::HashMap;
use std::fmt::Display;

/// Represents a specific difference between Source and Target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diff {
    /// The field exists in Target but is missing in Source.
    MissingField {
        /// Name of the missing field.
        name: String,
        /// Expected Rust type.
        ty: String,
    },

    /// The field exists in both but the types differ.
    TypeMismatch {
        /// Name of the field.
        field: String,
        /// Type found in Source.
        found: String,
        /// Type expected by Target.
        expected: String,
    },

    /// The field appears to be the same logical entity (same JSON name) but has a different Rust name.
    Renamed {
        /// The name in the Source code.
        source_name: String,
        /// The name in the Target definition.
        target_name: String,
        /// The stable serialization name linking them.
        json_name: String,
    },
}

impl Display for Diff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Diff::MissingField { name, ty } => write!(f, "Missing field '{}': {}", name, ty),
            Diff::TypeMismatch {
                field,
                found,
                expected,
            } => {
                write!(
                    f,
                    "Type mismatch on '{}': found '{}', expected '{}'",
                    field, found, expected
                )
            }
            Diff::Renamed {
                source_name,
                target_name,
                json_name,
            } => {
                write!(
                    f,
                    "Field '{}' renamed to '{}' (keyed by '{}')",
                    source_name, target_name, json_name
                )
            }
        }
    }
}

/// Calculates the difference between existing Source Code and a Target Definition.
pub fn calculate_diff(
    source_code: &str,
    struct_name: &str,
    target: &ParsedStruct,
) -> AppResult<Vec<Diff>> {
    let source_struct = extract_struct(source_code, struct_name)?;
    let mut diffs = Vec::new();

    let source_by_name: HashMap<&String, &ParsedField> =
        source_struct.fields.iter().map(|f| (&f.name, f)).collect();

    let source_by_json: HashMap<String, &ParsedField> = source_struct
        .fields
        .iter()
        .map(|f| (get_effective_name(f), f))
        .collect();

    for target_field in &target.fields {
        let target_rust_name = &target_field.name;
        let target_json_name = get_effective_name(target_field);

        if let Some(source_field) = source_by_name.get(target_rust_name) {
            let src_ty = strip_ws(&source_field.ty);
            let tgt_ty = strip_ws(&target_field.ty);

            if src_ty != tgt_ty {
                diffs.push(Diff::TypeMismatch {
                    field: target_rust_name.clone(),
                    found: source_field.ty.clone(),
                    expected: target_field.ty.clone(),
                });
            }
        } else if let Some(source_field) = source_by_json.get(&target_json_name) {
            diffs.push(Diff::Renamed {
                source_name: source_field.name.clone(),
                target_name: target_rust_name.clone(),
                json_name: target_json_name,
            });
        } else {
            diffs.push(Diff::MissingField {
                name: target_rust_name.clone(),
                ty: target_field.ty.clone(),
            });
        }
    }

    Ok(diffs)
}

fn get_effective_name(f: &ParsedField) -> String {
    f.rename.clone().unwrap_or_else(|| f.name.clone())
}

fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, ty: &str) -> ParsedField {
        ParsedField {
            name: name.to_string(),
            ty: ty.to_string(),
            description: None,
            rename: None,
            is_skipped: false,
            is_deprecated: false,
            external_docs: None,
        }
    }

    fn field_renamed(name: &str, ty: &str, rename: &str) -> ParsedField {
        ParsedField {
            name: name.to_string(),
            ty: ty.to_string(),
            description: None,
            rename: Some(rename.to_string()),
            is_skipped: false,
            is_deprecated: false,
            external_docs: None,
        }
    }

    #[test]
    fn test_perfect_match() {
        let code = "struct User { id: i32 }";
        let target = ParsedStruct {
            name: "User".into(),
            description: None,
            rename: None,
            rename_all: None,
            fields: vec![field("id", "i32")],
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
        };

        let diffs = calculate_diff(code, "User", &target).unwrap();
        assert!(diffs.is_empty());
    }

    #[test]
    fn test_missing_field() {
        let code = "struct User { id: i32 }";
        let target = ParsedStruct {
            name: "User".into(),
            description: None,
            rename: None,
            rename_all: None,
            fields: vec![field("id", "i32"), field("email", "String")],
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
        };

        let diffs = calculate_diff(code, "User", &target).unwrap();
        assert_eq!(diffs.len(), 1);
        if let Diff::MissingField { name, ty } = &diffs[0] {
            assert_eq!(name, "email");
            assert_eq!(ty, "String");
        } else {
            panic!("Wrong diff type");
        }
    }

    #[test]
    fn test_type_mismatch() {
        let code = "struct User { id: i64 }";
        let target = ParsedStruct {
            name: "User".into(),
            description: None,
            rename: None,
            rename_all: None,
            fields: vec![field("id", "i32")],
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
        };

        let diffs = calculate_diff(code, "User", &target).unwrap();
        assert_eq!(diffs.len(), 1);
    }

    #[test]
    fn test_renamed_detection() {
        let code = r#"
            struct User {
                #[serde(rename="id")]
                uid: i32
            }
        "#;

        let target = ParsedStruct {
            name: "User".into(),
            description: None,
            rename: None,
            rename_all: None,
            fields: vec![field_renamed("user_id", "i32", "id")],
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
        };

        let diffs = calculate_diff(code, "User", &target).unwrap();
        assert_eq!(diffs.len(), 1);
    }

    #[test]
    fn test_ignore_whitespace_types() {
        let code = "struct A { x: Option< String > }";
        let target = ParsedStruct {
            name: "A".into(),
            description: None,
            rename: None,
            rename_all: None,
            fields: vec![field("x", "Option<String>")],
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
        };

        let diffs = calculate_diff(code, "A", &target).unwrap();
        assert!(diffs.is_empty());
    }
}
