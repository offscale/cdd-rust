#![deny(missing_docs)]

//! # Struct Flattening
//!
//! Parsing logic to handle `Schema::Object` and `Schema::AllOf`.

use crate::error::AppResult;
use crate::oas::resolver::map_schema_to_rust_type;
use crate::oas::schemas::refs::{extract_ref_name, resolve_ref_name, ResolutionContext};
use crate::parser::ParsedField;
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
fn collect_fields_recursive(
    schema: &Schema,
    context: &ResolutionContext,
    fields: &mut Vec<ParsedField>,
    visited: &mut HashSet<String>,
) -> AppResult<()> {
    match schema {
        Schema::Object(obj) => {
            // Check type. If it's valid object or unspecified (inference), read properties.
            if matches!(obj.schema_type, SchemaType::Type(Type::Object))
                || obj.schema_type == SchemaType::AnyValue
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
                            let map_type =
                                format!("std::collections::HashMap<String, {}>", inner_type);

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

#[derive(Default)]
struct FieldMetadata {
    description: Option<String>,
    deprecated: bool,
    read_only: bool,
    write_only: bool,
}

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
    use crate::oas::schemas::refs::ResolutionContext;
    use utoipa::openapi::{OpenApi, RefOr};

    #[test]
    fn test_flatten_object_with_additional_properties() {
        let yaml = r#"
openapi: 3.1.0
info: {title: T, version: 1.0}
paths: {}
components:
  schemas:
    MapHolder:
      type: object
      additionalProperties: true
"#;
        let openapi: OpenApi = serde_yaml::from_str(yaml).unwrap();
        let components = openapi.components.as_ref().unwrap();
        let ctx = ResolutionContext::new(None, components);

        let schema = match components.schemas.get("MapHolder").unwrap() {
            RefOr::T(s) => s,
            RefOr::Ref(_) => panic!("Expected inline schema"),
        };

        let fields = flatten_schema_fields(schema, &ctx).unwrap();
        let addl = fields
            .iter()
            .find(|f| f.name == "additional_properties")
            .expect("missing additional_properties");
        assert!(addl.ty.contains("HashMap"));
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
        let openapi: OpenApi = serde_yaml::from_str(yaml).unwrap();
        let components = openapi.components.as_ref().unwrap();
        let ctx = ResolutionContext::new(None, components);

        let schema = match components.schemas.get("Combined").unwrap() {
            RefOr::T(s) => s,
            RefOr::Ref(_) => panic!("Expected inline schema"),
        };

        let fields = flatten_schema_fields(schema, &ctx).unwrap();
        let id = fields.iter().find(|f| f.name == "id").unwrap();
        let note = fields.iter().find(|f| f.name == "note").unwrap();
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
        let openapi: OpenApi = serde_yaml::from_str(yaml).unwrap();
        let components = openapi.components.as_ref().unwrap();
        let ctx = ResolutionContext::new(None, components);

        let schema = match components.schemas.get("User").unwrap() {
            RefOr::T(s) => s,
            RefOr::Ref(_) => panic!("Expected inline schema"),
        };

        let fields = flatten_schema_fields(schema, &ctx).unwrap();
        let id = fields.iter().find(|f| f.name == "id").unwrap();
        let password = fields.iter().find(|f| f.name == "password").unwrap();
        let status = fields.iter().find(|f| f.name == "status").unwrap();

        assert!(id.is_read_only);
        assert!(!id.is_write_only);
        assert!(password.is_write_only);
        assert!(!password.is_read_only);
        assert_eq!(status.description.as_deref(), Some("Status description"));
        assert!(status.is_deprecated);
        assert!(status.is_read_only);
    }
}
