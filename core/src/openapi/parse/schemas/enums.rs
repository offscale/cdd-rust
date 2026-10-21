#![deny(missing_docs)]

//! # Enum Parsing
//!
//! Logic for parsing `oneOf` or `anyOf` schemas into Rust Enums.

use crate::classes::parse::ParsedVariant;
use crate::openapi::parse::resolver::map_schema_to_rust_type;
use crate::openapi::parse::schemas::refs::extract_ref_name;
use std::collections::BTreeMap;
use utoipa::openapi::{schema::Schema, Deprecated, RefOr};

/// Helpers to extract variants from RefOr list (used by OneOf and AnyOf).
///
/// # Arguments
///
/// * `items` - The list of schemas in the oneOf/anyOf block.
/// * `mapping` - The optional discriminator mapping from the parent schema.
pub(crate) fn parse_variants(
    items: &[RefOr<Schema>],
    mapping: Option<&BTreeMap<String, String>>,
) -> Vec<ParsedVariant> {
    let mut variants = Vec::new();

    for (index, item) in items.iter().enumerate() {
        // Determine Variant Name, Type, and Deprecation
        let (variant_auto_name, type_name, is_deprecated) = match item {
            RefOr::Ref(r) => {
                let name = extract_ref_name(&r.ref_location);
                // Ref doesn't carry deprecation info directly in this context (requires resolution)
                (name.clone(), Some(name), false)
            }
            RefOr::T(s) => {
                // Inline schema
                let deprecated = matches!(
                    s,
                    Schema::Object(utoipa::openapi::schema::Object {
                        deprecated: Some(Deprecated::True),
                        ..
                    })
                );

                let ty = map_schema_to_rust_type(&RefOr::T(s.clone()), true)
                    .expect("map_schema_to_rust_type is infallible");
                let simple_name =
                    derive_name_from_type(&ty).unwrap_or_else(|| format!("Variant{}", index));
                (simple_name, Some(ty), deprecated)
            }
        };

        // Determine if there is a discriminator mapping to override the auto name
        let mut rename = None;
        let mut aliases = Vec::new();

        if let Some(map) = mapping {
            // Only Refs can be reliably mapped via reference target matching in this static analyzer
            if let RefOr::Ref(r) = item {
                let ref_target_name = extract_ref_name(&r.ref_location);
                for (mapped_key, mapped_ref) in map {
                    let mapped_target = extract_ref_name(mapped_ref);
                    // Matches if the Ref in oneOf points to "Cat" and mapping points to "Cat"
                    if mapped_target == ref_target_name {
                        if rename.is_none() {
                            rename = Some(mapped_key.clone());
                        } else {
                            aliases.push(mapped_key.clone());
                        }
                    }
                }
            }
        }

        let variant_name = variant_auto_name;

        variants.push(ParsedVariant {
            name: variant_name,
            ty: type_name,
            description: None,
            rename,
            aliases: Some(aliases),
            is_deprecated,
        });
    }
    variants
}

/// Heuristic to name variants based on Rust types.
fn derive_name_from_type(ty: &str) -> Option<String> {
    match ty {
        "String" => Some("String".to_string()),
        "i32" | "i64" | "u32" | "u64" | "usize" | "isize" => Some("Integer".to_string()),
        "f32" | "f64" => Some("Number".to_string()),
        "bool" => Some("Boolean".to_string()),
        "Uuid" => Some("Uuid".to_string()),
        "NaiveDate" => Some("Date".to_string()),
        "DateTime" => Some("DateTime".to_string()),
        val if val.starts_with("Vec<") => Some("Array".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::openapi::schema::{ObjectBuilder, Type};
    use utoipa::openapi::{Ref, RefOr};

    #[test]
    fn test_parse_variants_with_mapping() {
        let items = vec![RefOr::Ref(Ref::new("#/components/schemas/Cat"))];
        let mut mapping = BTreeMap::new();
        mapping.insert("cat".to_string(), "#/components/schemas/Cat".to_string());
        mapping.insert("kitty".to_string(), "#/components/schemas/Cat".to_string());

        let variants = parse_variants(&items, Some(&mapping));
        assert_eq!(variants.len(), 1);
        let v = &variants[0];
        assert_eq!(v.name, "Cat");
        assert_eq!(v.rename.as_deref(), Some("cat"));
        assert_eq!(
            v.aliases.as_ref().expect("Failed to get reference"),
            &vec!["kitty".to_string()]
        );
    }

    #[test]
    fn test_parse_variants_inline_schema() {
        let schema = Schema::Object(ObjectBuilder::new().schema_type(Type::String).build());
        let deprecated_schema = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::Integer)
                .deprecated(Some(Deprecated::True))
                .build(),
        );
        let items = vec![RefOr::T(schema), RefOr::T(deprecated_schema)];
        let variants = parse_variants(&items, None);
        assert_eq!(variants.len(), 2);
        let v = &variants[0];
        assert_eq!(v.name, "String");
        assert_eq!(v.ty.as_deref(), Some("String"));
        assert!(v.rename.is_none());
        assert!(!v.is_deprecated);
        assert!(variants[1].is_deprecated);
    }

    #[test]
    fn test_derive_name_from_type() {
        assert_eq!(derive_name_from_type("String"), Some("String".to_string()));
        assert_eq!(derive_name_from_type("i32"), Some("Integer".to_string()));
        assert_eq!(derive_name_from_type("f64"), Some("Number".to_string()));
        assert_eq!(derive_name_from_type("bool"), Some("Boolean".to_string()));
        assert_eq!(derive_name_from_type("Uuid"), Some("Uuid".to_string()));
        assert_eq!(derive_name_from_type("NaiveDate"), Some("Date".to_string()));
        assert_eq!(
            derive_name_from_type("DateTime"),
            Some("DateTime".to_string())
        );
        assert_eq!(
            derive_name_from_type("Vec<User>"),
            Some("Array".to_string())
        );
        assert_eq!(derive_name_from_type("Unknown"), None);

        // Test fallback Variant{idx} on inline Object schema and mapping with inline schema + non-matching ref
        let inline_obj = Schema::Object(ObjectBuilder::new().schema_type(Type::Object).build());
        let mut mapping = BTreeMap::new();
        mapping.insert(
            "other".to_string(),
            "#/components/schemas/Other".to_string(),
        );
        let items = vec![
            RefOr::T(inline_obj),
            RefOr::Ref(Ref::new("#/components/schemas/Cat")),
        ];
        let variants = parse_variants(&items, Some(&mapping));
        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].name, "Variant0");
        assert_eq!(variants[1].name, "Cat");
        assert!(variants[1].rename.is_none());
    }
}
