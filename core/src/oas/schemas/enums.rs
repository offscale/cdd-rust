#![deny(missing_docs)]

//! # Enum Parsing
//!
//! Logic for parsing `oneOf` or `anyOf` schemas into Rust Enums.

use crate::oas::resolver::map_schema_to_rust_type;
use crate::oas::schemas::refs::extract_ref_name;
use crate::parser::ParsedVariant;
use std::collections::BTreeMap;
use utoipa::openapi::{schema::Schema, RefOr};

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

    for item in items {
        let variant_ref_name = match item {
            RefOr::Ref(r) => extract_ref_name(&r.ref_location),
            RefOr::T(_) => "AnonymousVariant".to_string(),
        };

        // By default, variant name matches the referenced component name
        let variant_name = variant_ref_name.clone();

        // Map the discriminator value to this variant if a 'mapping' exists in spec.
        // We support multiple mapping keys pointing to the same schema by using renaming + aliases.
        let mut rename = None;
        let mut aliases = Vec::new();

        if let Some(map) = mapping {
            // Iterate over all entries in the mapping to find matches.
            // The mapping format is: "DiscriminatorValue": "SchemaReference"
            for (mapped_key, mapped_ref) in map {
                let mapped_target = extract_ref_name(mapped_ref);

                // If the mapping target matches this variant's reference name
                if mapped_target == variant_ref_name {
                    if rename.is_none() {
                        // First match becomes the primary serde rename
                        rename = Some(mapped_key.clone());
                    } else {
                        // Subsequent matches become aliases
                        aliases.push(mapped_key.clone());
                    }
                }
            }
        }

        let type_name = if let RefOr::Ref(_) = item {
            Some(variant_name.clone())
        } else {
            // Inline schema in OneOf/AnyOf - try to map to basic type or Value
            match item {
                RefOr::T(s) => match map_schema_to_rust_type(&RefOr::T(s.clone()), true) {
                    Ok(t) => Some(t),
                    Err(_) => Some("serde_json::Value".to_string()),
                },
                _ => None,
            }
        };

        variants.push(ParsedVariant {
            name: variant_name,
            ty: type_name,
            description: None,
            rename,
            aliases: Some(aliases),
        });
    }
    variants
}
