#![deny(missing_docs)]

//! # Struct Flattening
//!
//! Parsing logic to handle `Schema::Object` and `Schema::AllOf`.

use crate::classes::parse::ParsedField;
use crate::error::AppResult;
use crate::openapi::parse::resolver::map_schema_to_rust_type;
use crate::openapi::parse::schemas::refs::{extract_ref_name, resolve_ref_name, ResolutionContext};
use std::collections::HashSet;
use utoipa::openapi::{
    schema::{Schema, SchemaType, Type},
    Deprecated, RefOr,
};

/// Recursively gathers fields from a schema, handling `allOf` flattening and `Ref` lookup.
///
/// # Arguments
///
/// * `root_schema` - The schema definition to process (Object or AllOf).
/// * `context` - Resolution context containing components and base URI.
pub(crate) fn flatten_schema_fields(
    root_schema: &Schema,
    context: &ResolutionContext,
) -> AppResult<Vec<ParsedField>> {
    // Use Vec to preserve order, handle overrides manually
    let mut fields: Vec<ParsedField> = Vec::new();
    let mut visited_refs = HashSet::new();

    collect_fields_recursive(root_schema, context, &mut fields, &mut visited_refs)?;

    Ok(fields)
}

/// Helper for recursive field merging.
///
/// Logic:
/// - If `Object`: extract properties. Check if field already exists in accumulator; if so, replace it (override).
/// - If `AllOf`: recursively collect for all items. Rules of overrides apply sequentially.
/// - If `additionalProperties` is present: add a flattened HashMap field.
///
/// # Arguments
/// * `schema` - Input `schema` parameter.
/// * `context` - Input `context` parameter.
/// * `fields` - Input `fields` parameter.
/// * `visited` - Input `visited` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
fn collect_fields_recursive(
    schema: &Schema,
    context: &ResolutionContext,
    fields: &mut Vec<ParsedField>,
    visited: &mut HashSet<String>,
) -> AppResult<()> {
    match schema {
        Schema::Object(obj)
            if matches!(
                obj.schema_type,
                SchemaType::Type(Type::Object) | SchemaType::AnyValue
            ) =>
        {
            // 1. Explicit Properties
            for (field_name, field_schema) in &obj.properties {
                let is_required = obj.required.contains(field_name);
                let rust_type = map_schema_to_rust_type(field_schema, is_required)?;

                // Extract metadata
                // Note: utoipa::openapi::Object does not currently expose externalDocs
                let metadata = extract_field_metadata(field_schema, context);

                let field = ParsedField {
                    name: field_name.clone(),
                    ty: rust_type,
                    description: metadata.description,
                    rename: None,
                    is_skipped: false,
                    is_read_only: metadata.read_only,
                    is_write_only: metadata.write_only,
                    is_deprecated: metadata.deprecated,
                    external_docs: None, // Not available in current Schema Object model
                };

                // Upsert mechanism to handle overrides
                if let Some(idx) = fields.iter().position(|f| f.name == *field_name) {
                    fields[idx] = field;
                } else {
                    fields.push(field);
                }
            }

            // 2. Additional Properties (Map)
            if let Some(add_props) = &obj.additional_properties {
                match &**add_props {
                    // additionalProperties: false -> no extra field (deny unknowns)
                    utoipa::openapi::schema::AdditionalProperties::FreeForm(false) => {}
                    // additionalProperties: true -> HashMap<String, Value>
                    utoipa::openapi::schema::AdditionalProperties::FreeForm(true) => {
                        let map_type =
                            "std::collections::HashMap<String, serde_json::Value>".to_string();
                        let parsed_field = ParsedField {
                            name: "additional_properties".to_string(),
                            ty: map_type,
                            description: Some("Captured additional properties".to_string()),
                            rename: None,
                            is_skipped: false,
                            is_read_only: false,
                            is_write_only: false,
                            is_deprecated: false,
                            external_docs: None,
                        };

                        if !fields.iter().any(|f| f.name == "additional_properties") {
                            fields.push(parsed_field);
                        }
                    }
                    // additionalProperties: { schema } -> HashMap<String, SchemaType>
                    utoipa::openapi::schema::AdditionalProperties::RefOr(schema) => {
                        let inner_type = map_schema_to_rust_type(schema, true)?;
                        let map_type = format!("std::collections::HashMap<String, {}>", inner_type);

                        let parsed_field = ParsedField {
                            name: "additional_properties".to_string(),
                            ty: map_type,
                            description: Some("Captured additional properties".to_string()),
                            rename: None,
                            is_skipped: false,
                            is_read_only: false,
                            is_write_only: false,
                            is_deprecated: false,
                            external_docs: None,
                        };

                        if !fields.iter().any(|f| f.name == "additional_properties") {
                            fields.push(parsed_field);
                        }
                    }
                }
            }
        }
        Schema::AllOf(all_of) => {
            // Iterate items and merge
            for item in &all_of.items {
                match item {
                    RefOr::T(s) => collect_fields_recursive(s, context, fields, visited)?,
                    RefOr::Ref(r) => {
                        let ref_name = extract_ref_name(&r.ref_location);

                        // Cycle detection
                        if visited.contains(&ref_name) {
                            continue; // Skip cycle
                        }
                        visited.insert(ref_name.clone());

                        // Use Context-aware resolution
                        if let Some(resolved) = resolve_ref_name(&r.ref_location, context) {
                            collect_fields_recursive(resolved, context, fields, visited)?;
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

/// Internal representation for `FieldMetadata` (field metadata).
#[derive(Default)]
struct FieldMetadata {
    /// Field `description` (description).
    description: Option<String>,
    /// Field `deprecated` (deprecated).
    deprecated: bool,
    /// Field `read_only` (read only).
    read_only: bool,
    /// Field `write_only` (write only).
    write_only: bool,
}

/// Executes `extract_field_metadata` (extract field metadata).
///
/// # Arguments
/// * `schema` - Input `schema` parameter.
/// * `context` - Input `context` parameter.
///
/// # Returns
/// Resulting `FieldMetadata` value.
fn extract_field_metadata(schema: &RefOr<Schema>, context: &ResolutionContext) -> FieldMetadata {
    let resolved = match schema {
        RefOr::T(s) => Some(s),
        RefOr::Ref(r) => resolve_ref_name(&r.ref_location, context),
    };

    match resolved {
        Some(Schema::Object(obj)) => FieldMetadata {
            description: obj.description.clone(),
            deprecated: matches!(obj.deprecated, Some(Deprecated::True)),
            read_only: obj.read_only.unwrap_or(false),
            write_only: obj.write_only.unwrap_or(false),
        },
        Some(Schema::AllOf(all_of)) => FieldMetadata {
            description: all_of.description.clone(),
            deprecated: false,
            read_only: false,
            write_only: false,
        },
        _ => FieldMetadata::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::schemas::refs::ResolutionContext;
    use utoipa::openapi::{OpenApi, RefOr};

    #[test]
    fn test_flatten_object_with_additional_properties() {
        let yaml = r#"
openapi: 3.1.0
info: {title: T, version: 1.0}
paths: {}
components:
  schemas:
    RefHolder:
      $ref: '#/components/schemas/MapHolder'
    MapHolder:
      type: object
      additionalProperties: true
"#;
        let openapi: OpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let components = openapi
            .components
            .as_ref()
            .expect("Failed to get reference");
        let ctx = ResolutionContext::new(None, components);

        for s in components.schemas.values() {
            if let RefOr::T(schema) = s {
                let fields =
                    flatten_schema_fields(schema, &ctx).expect("Failed to flatten schema fields");
                let addl = fields
                    .iter()
                    .find(|f| f.name == "additional_properties")
                    .expect("missing additional_properties");
                assert!(addl.ty.contains("HashMap"));
            }
        }
    }

    #[test]
    fn test_flatten_all_of_with_override() {
        let yaml = r#"
openapi: 3.1.0
info: {title: T, version: 1.0}
paths: {}
components:
  schemas:
    Base:
      type: object
      properties:
        id: { type: string }
    Extra:
      type: object
      properties:
        note: { type: string }
    Combined:
      allOf:
        - $ref: '#/components/schemas/Base'
        - $ref: '#/components/schemas/Extra'
        - type: object
          properties:
            id: { type: integer }
          required: [id]
"#;
        let openapi: OpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let components = openapi
            .components
            .as_ref()
            .expect("Failed to get reference");
        let ctx = ResolutionContext::new(None, components);

        let all_fields: Vec<_> = components
            .schemas
            .iter()
            .filter_map(|(k, v)| match (k.as_str(), v) {
                ("Combined", RefOr::T(s)) => flatten_schema_fields(s, &ctx).ok(),
                _ => None,
            })
            .flatten()
            .collect();
        let id = all_fields
            .iter()
            .find(|f| f.name == "id")
            .expect("Failed to find");
        let note = all_fields
            .iter()
            .find(|f| f.name == "note")
            .expect("Failed to find");
        assert_eq!(id.ty, "i32");
        assert_eq!(note.ty, "Option<String>");
    }

    #[test]
    fn test_field_metadata_read_write_only_and_ref_description() {
        let yaml = r#"
openapi: 3.1.0
info: {title: T, version: 1.0}
paths: {}
components:
  schemas:
    Status:
      type: string
      description: Status description
      deprecated: true
      readOnly: true
    User:
      type: object
      properties:
        id:
          type: string
          readOnly: true
        password:
          type: string
          writeOnly: true
        status:
          $ref: '#/components/schemas/Status'
"#;
        let openapi: OpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let components = openapi
            .components
            .as_ref()
            .expect("Failed to get reference");
        let ctx = ResolutionContext::new(None, components);

        let fields: Vec<_> = components
            .schemas
            .iter()
            .filter_map(|(k, v)| match (k.as_str(), v) {
                ("User", RefOr::T(s)) => flatten_schema_fields(s, &ctx).ok(),
                _ => None,
            })
            .flatten()
            .collect();
        let id = fields
            .iter()
            .find(|f| f.name == "id")
            .expect("Failed to find");
        let password = fields
            .iter()
            .find(|f| f.name == "password")
            .expect("Failed to find");
        let status = fields
            .iter()
            .find(|f| f.name == "status")
            .expect("Failed to find");

        assert!(id.is_read_only);
        assert!(!id.is_write_only);
        assert!(password.is_write_only);
        assert!(!password.is_read_only);
        assert_eq!(status.description.as_deref(), Some("Status description"));
        assert!(status.is_deprecated);
        assert!(status.is_read_only);

        // Cover AdditionalProperties::FreeForm(false), AdditionalProperties::RefOr, duplicate additional_properties,
        // cyclic allOf, unresolved allOf ref, Schema::AllOf field metadata, and non-object/non-AllOf schema
        let edge_yaml = r#"
openapi: 3.1.0
info: {title: T, version: 1.0}
paths: {}
components:
  schemas:
    RefAlias:
      $ref: '#/components/schemas/TypedMap'
    CycleA:
      allOf:
        - $ref: '#/components/schemas/CycleA'
        - $ref: '#/components/schemas/Missing'
        - type: string
        - type: object
          additionalProperties: true
        - type: object
          additionalProperties: true
        - type: object
          additionalProperties: false
        - type: object
          additionalProperties:
            type: integer
    TypedMap:
      allOf:
        - type: object
          additionalProperties:
            type: string
        - type: object
          additionalProperties:
            type: integer
        - type: object
          properties:
            all_of_prop:
              allOf:
                - type: object
              description: AllOf prop desc
            one_of_prop:
              oneOf:
                - type: string
            missing_ref_prop:
              $ref: '#/components/schemas/Missing'
            additional_properties:
              type: string
"#;
        let edge_api: OpenApi = serde_yaml::from_str(edge_yaml).expect("edge yaml");
        let edge_comps = edge_api.components.as_ref().expect("edge comps");
        let edge_ctx = ResolutionContext::new(None, edge_comps);

        for (name, schema_ref) in &edge_comps.schemas {
            if let RefOr::T(s) = schema_ref {
                let res = flatten_schema_fields(s, &edge_ctx).expect("fields");
                if name == "CycleA" {
                    assert_eq!(res.len(), 1);
                } else {
                    let aop = res.iter().find(|f| f.name == "all_of_prop").expect("aop");
                    assert_eq!(aop.description.as_deref(), Some("AllOf prop desc"));
                }
            }
        }
    }
}
