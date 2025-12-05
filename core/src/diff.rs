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
///
/// # Arguments
///
/// * `source_code` - The raw Rust existing code.
/// * `struct_name` - The name of the struct to compare.
/// * `target` - The desired struct definition (e.g. from DB or Schema).
///
/// # Returns
///
/// * `Vec<Diff>` - A list of discrepancies.
pub fn calculate_diff(
    source_code: &str,
    struct_name: &str,
    target: &ParsedStruct,
) -> AppResult<Vec<Diff>> {
    // 1. Parse the existing source to understand current state
    let source_struct = extract_struct(source_code, struct_name)?;

    let mut diffs = Vec::new();

    // Index source fields for lookups
    // Map: RustName -> ParsedField
    let source_by_name: HashMap<&String, &ParsedField> =
        source_struct.fields.iter().map(|f| (&f.name, f)).collect();

    // Map: EffectiveJsonName -> ParsedField
    // Used to detect renames where logical ID stays same but Rust name differs
    let source_by_json: HashMap<String, &ParsedField> = source_struct
        .fields
        .iter()
        .map(|f| (get_effective_name(f), f))
        .collect();

    for target_field in &target.fields {
        let target_rust_name = &target_field.name;
        let target_json_name = get_effective_name(target_field);

        // Check 1: Exact Rust Name Match
        if let Some(source_field) = source_by_name.get(target_rust_name) {
            // Field exists. Check Type.
            // We strip whitespace to avoid diffs on "Option<T>" vs "Option < T >"
            let src_ty = strip_ws(&source_field.ty);
            let tgt_ty = strip_ws(&target_field.ty);

            if src_ty != tgt_ty {
                diffs.push(Diff::TypeMismatch {
                    field: target_rust_name.clone(),
                    found: source_field.ty.clone(),
                    expected: target_field.ty.clone(),
                });
            }
        } else {
            // Check 2: Logical Match (Rename detection)
            // If we can't find it by Rust name, does the JSON name match existing field?
            if let Some(source_field) = source_by_json.get(&target_json_name) {
                // We found a field that serializes to the same key, but has different Rust name.
                diffs.push(Diff::Renamed {
                    source_name: source_field.name.clone(),
                    target_name: target_rust_name.clone(),
                    json_name: target_json_name,
                });

                // We could also check type mismatch here for the renamed field,
                // but usually Rename is the primary diff of interest.
            } else {
                // Check 3: Truly Missing
                diffs.push(Diff::MissingField {
                    name: target_rust_name.clone(),
                    ty: target_field.ty.clone(),
                });
            }
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

    // Helper to build a field manually
    fn field(name: &str, ty: &str) -> ParsedField {
        ParsedField {
            name: name.to_string(),
            ty: ty.to_string(),
            description: None,
            rename: None,
            is_skipped: false,
        }
    }

    fn field_renamed(name: &str, ty: &str, rename: &str) -> ParsedField {
        ParsedField {
            name: name.to_string(),
            ty: ty.to_string(),
            description: None,
            rename: Some(rename.to_string()),
            is_skipped: false,
        }
    }

    #[test]
    fn test_perfect_match() {
        let code = "struct User { id: i32 }";
        let target = ParsedStruct {
            name: "User".into(),
            description: None,
            rename: None,
            fields: vec![field("id", "i32")],
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
            fields: vec![field("id", "i32"), field("email", "String")],
        };

        let diffs = calculate_diff(code, "User", &target).unwrap();
        assert_eq!(diffs.len(), 1);
        match &diffs[0] {
            Diff::MissingField { name, ty } => {
                assert_eq!(name, "email");
                assert_eq!(ty, "String");
            }
            _ => panic!("Wrong diff type"),
        }
    }

    #[test]
    fn test_type_mismatch() {
        let code = "struct User { id: i64 }";
        let target = ParsedStruct {
            name: "User".into(),
            description: None,
            rename: None,
            fields: vec![field("id", "i32")],
        };

        let diffs = calculate_diff(code, "User", &target).unwrap();
        assert_eq!(diffs.len(), 1);
        if let Diff::TypeMismatch {
            field,
            found,
            expected,
        } = &diffs[0]
        {
            assert_eq!(field, "id");
            assert_eq!(found, "i64");
            assert_eq!(expected, "i32");
        } else {
            panic!("Wrong diff");
        }
    }

    #[test]
    fn test_renamed_detection() {
        // Source has 'uid' mapped to 'id'.
        // Target wants 'user_id' mapped to 'id'.
        // This implies the Logical field 'id' exists, but Rust variable is named differently.
        let code = r#"
            struct User {
                #[serde(rename="id")]
                uid: i32
            }
        "#;

        // Target definition expects: 'user_id' -> 'id'
        let target = ParsedStruct {
            name: "User".into(),
            description: None,
            rename: None,
            fields: vec![field_renamed("user_id", "i32", "id")],
        };

        let diffs = calculate_diff(code, "User", &target).unwrap();

        assert_eq!(diffs.len(), 1);
        if let Diff::Renamed {
            source_name,
            target_name,
            json_name,
        } = &diffs[0]
        {
            assert_eq!(source_name, "uid"); // Code has uid
            assert_eq!(target_name, "user_id"); // Target wants user_id
            assert_eq!(json_name, "id"); // Key
        } else {
            panic!("Expected Renamed diff");
        }
    }

    #[test]
    fn test_ignore_whitespace_types() {
        let code = "struct A { x: Option< String > }";
        let target = ParsedStruct {
            name: "A".into(),
            description: None,
            rename: None,
            fields: vec![field("x", "Option<String>")], // No spaces
        };

        let diffs = calculate_diff(code, "A", &target).unwrap();
        assert!(diffs.is_empty());
    }

    #[test]
    fn test_display_impl() {
        let d = Diff::MissingField {
            name: "x".into(),
            ty: "int".into(),
        };
        assert_eq!(format!("{}", d), "Missing field 'x': int");
    }

    #[test]
    fn test_display_type_mismatch() {
        let d = Diff::TypeMismatch {
            field: "x".into(),
            found: "i32".into(),
            expected: "i64".into(),
        };
        assert_eq!(
            format!("{}", d),
            "Type mismatch on 'x': found 'i32', expected 'i64'"
        );
    }

    #[test]
    fn test_display_renamed() {
        let d = Diff::Renamed {
            source_name: "src".into(),
            target_name: "tgt".into(),
            json_name: "key".into(),
        };
        assert_eq!(
            format!("{}", d),
            "Field 'src' renamed to 'tgt' (keyed by 'key')"
        );
    }
}
