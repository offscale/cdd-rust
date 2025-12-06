#![deny(missing_docs)]

//! # Struct Flattening
//!
//! Parsing logic to handle `Schema::Object` and `Schema::AllOf`.

use crate::error::AppResult;
use crate::oas::resolver::map_schema_to_rust_type;
use crate::oas::schemas::refs::{extract_ref_name, resolve_ref_name};
use crate::parser::ParsedField;
use std::collections::HashSet;
use utoipa::openapi::{
    schema::{Schema, SchemaType, Type},
    Components, RefOr,
};

/// Recursively gathers fields from a schema, handling `allOf` flattening and `Ref` lookup.
///
/// # Arguments
///
/// * `root_schema` - The schema definition to process (Object or AllOf).
/// * `components` - Global definitions for resolving references.
pub(crate) fn flatten_schema_fields(
    root_schema: &Schema,
    components: &Components,
) -> AppResult<Vec<ParsedField>> {
    // Use Vec to preserve order, handle overrides manually
    let mut fields: Vec<ParsedField> = Vec::new();
    let mut visited_refs = HashSet::new();

    collect_fields_recursive(root_schema, components, &mut fields, &mut visited_refs)?;

    Ok(fields)
}

/// Helper for recursive field merging.
///
/// Logic:
/// - If `Object`: extract properties. Check if field already exists in accumulator; if so, replace it (override).
/// - If `AllOf`: recursively collect for all items. Rules of overrides apply sequentially.
fn collect_fields_recursive(
    schema: &Schema,
    components: &Components,
    fields: &mut Vec<ParsedField>,
    visited: &mut HashSet<String>,
) -> AppResult<()> {
    match schema {
        Schema::Object(obj) => {
            // Check type. If it's valid object or unspecified (inference), read properties.
            if matches!(obj.schema_type, SchemaType::Type(Type::Object))
                || obj.schema_type == SchemaType::AnyValue
            {
                for (field_name, field_schema) in &obj.properties {
                    let is_required = obj.required.contains(field_name);
                    let rust_type = map_schema_to_rust_type(field_schema, is_required)?;

                    let description = match field_schema {
                        RefOr::T(Schema::Object(o)) => o.description.clone(),
                        RefOr::T(Schema::AllOf(a)) => a.description.clone(),
                        _ => None,
                    };

                    let field = ParsedField {
                        name: field_name.clone(),
                        ty: rust_type,
                        description,
                        rename: None,
                        is_skipped: false,
                    };

                    // Upsert mechanism to handle overrides
                    if let Some(idx) = fields.iter().position(|f| f.name == *field_name) {
                        fields[idx] = field;
                    } else {
                        fields.push(field);
                    }
                }
            }
        }
        Schema::AllOf(all_of) => {
            // Iterate items and merge
            for item in &all_of.items {
                match item {
                    RefOr::T(s) => collect_fields_recursive(s, components, fields, visited)?,
                    RefOr::Ref(r) => {
                        let ref_name = extract_ref_name(&r.ref_location);

                        // Cycle detection
                        if visited.contains(&ref_name) {
                            continue; // Skip cycle
                        }
                        visited.insert(ref_name.clone());

                        if let Some(resolved) = resolve_ref_name(&ref_name, components) {
                            collect_fields_recursive(resolved, components, fields, visited)?;
                        }

                        visited.remove(&ref_name);
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}
