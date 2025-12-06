#![deny(missing_docs)]

//! # Schema Parsing
//!
//! Handles parsing of OpenAPI `components/schemas`.
//!
//! Defines logic to:
//! - Extract struct/enum definitions.
//! - Flatten `allOf` inheritance/composition into single structs.
//! - Handle `oneOf` and `anyOf` polymorphism.
//! - Resolve `Ref` pointers within the schema scope.

use crate::error::{AppError, AppResult};
use crate::oas::resolver::map_schema_to_rust_type;
use crate::parser::{ParsedEnum, ParsedField, ParsedModel, ParsedStruct, ParsedVariant};
use std::collections::HashSet;
use utoipa::openapi::schema::{Schema, SchemaType, Type};
use utoipa::openapi::{Components, OpenApi, RefOr};

/// Parses a raw OpenAPI YAML string and extracts definitions.
///
/// Handles `allOf` flattening, `oneOf`/`anyOf` variants.
pub fn parse_openapi_spec(yaml_content: &str) -> AppResult<Vec<ParsedModel>> {
    let openapi: OpenApi = serde_yaml::from_str(yaml_content)
        .map_err(|e| AppError::General(format!("Failed to parse OpenAPI YAML: {}", e)))?;

    let components = openapi
        .components
        .as_ref()
        .ok_or_else(|| AppError::General("No components found in OpenAPI spec".into()))?;

    let mut models = Vec::new();

    {
        for (name, ref_or_schema) in &components.schemas {
            // Resolve local ref if usually top-level
            let schema = resolve_ref_local(ref_or_schema, components);

            if let Some(s) = schema {
                match s {
                    // Case 1: Simple Object or AllOf (Merged Struct)
                    Schema::Object(_) | Schema::AllOf(_) => {
                        let parsed_fields = flatten_schema_fields(s, components)?;

                        // Extract description if present on the top level
                        let description = match s {
                            Schema::Object(o) => o.description.clone(),
                            Schema::AllOf(a) => a.description.clone(),
                            _ => None,
                        };

                        models.push(ParsedModel::Struct(ParsedStruct {
                            name: name.clone(),
                            description,
                            rename: None,
                            fields: parsed_fields,
                        }));
                    }
                    // Case 2: OneOf (Enum)
                    Schema::OneOf(one_of) => {
                        let discriminator = one_of.discriminator.clone();
                        let tag = discriminator.as_ref().map(|d| d.property_name.clone());
                        // mapping is a BTreeMap, we access it directly via reference mapping
                        let mapping = discriminator.as_ref().map(|d| &d.mapping);
                        let is_untagged = tag.is_none();

                        let variants = parse_variants(&one_of.items, mapping);

                        if !variants.is_empty() {
                            models.push(ParsedModel::Enum(ParsedEnum {
                                name: name.clone(),
                                description: None,
                                rename: None,
                                tag,
                                untagged: is_untagged,
                                variants,
                            }));
                        }
                    }
                    // Case 3: AnyOf (Enum)
                    // Treated similarly to OneOf: untagged enum which validates "matches at least one".
                    // In Rust serde "untagged", this creates an enum that tries variants in order.
                    Schema::AnyOf(any_of) => {
                        // AnyOf also supports discriminator in OAS 3.x, though less common.
                        let discriminator = any_of.discriminator.clone();
                        let tag = discriminator.as_ref().map(|d| d.property_name.clone());
                        let mapping = discriminator.as_ref().map(|d| &d.mapping);

                        // Default anyOf to untagged unless strict discriminator is used
                        let is_untagged = tag.is_none();

                        let variants = parse_variants(&any_of.items, mapping);

                        if !variants.is_empty() {
                            models.push(ParsedModel::Enum(ParsedEnum {
                                name: name.clone(),
                                description: Some("AnyOf: Matches at least one schema".to_string()),
                                rename: None,
                                tag,
                                untagged: is_untagged,
                                variants,
                            }));
                        }
                    }
                    // Note: `not` schema is not currently supported by utoipa::openapi::Schema enum parsing.
                    // If supported in future updates, we would map it to a generic Value wrapper.
                    _ => {}
                }
            }
        }
    }

    Ok(models)
}

/// Helpers to extract variants from RefOr list (used by OneOf and AnyOf)
fn parse_variants(
    items: &[RefOr<Schema>],
    mapping: Option<&std::collections::BTreeMap<String, String>>,
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
        let mut rename = None;
        if let Some(map) = mapping {
            // Reverse lookup: Find Key where Value corresponds to this schema.
            // The mapping value is a Ref string (e.g. "#/components/schemas/Dog").
            for (mapped_key, mapped_ref) in map {
                let mapped_target = extract_ref_name(mapped_ref);
                if mapped_target == variant_ref_name {
                    rename = Some(mapped_key.clone());
                    break;
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
        });
    }
    variants
}

/// Recursively gathers fields from a schema, handling `allOf` flattening and `Ref` lookup.
fn flatten_schema_fields(
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

/// Resolves a `RefOr` to a `Schema` against `components`.
/// Only resolves one level deep to avoid infinite recursion loops in logic outside `collect_fields`.
fn resolve_ref_local<'a>(
    ref_or: &'a RefOr<Schema>,
    components: &'a Components,
) -> Option<&'a Schema> {
    match ref_or {
        RefOr::T(s) => Some(s),
        RefOr::Ref(r) => {
            let name = extract_ref_name(&r.ref_location);
            resolve_ref_name(&name, components)
        }
    }
}

/// Resolves a schema name (simple string) to a Schema object in components.
fn resolve_ref_name<'a>(name: &str, components: &'a Components) -> Option<&'a Schema> {
    components
        .schemas
        .get(name)
        .and_then(|ref_or| match ref_or {
            RefOr::T(s) => Some(s),
            RefOr::Ref(_) => None, // Double ref resolution avoided for simplicity
        })
}

/// Extracts the simple name from a reference string.
/// e.g. `#/components/schemas/User` -> `User`
fn extract_ref_name(ref_loc: &str) -> String {
    ref_loc
        .split('/')
        .next_back()
        .unwrap_or("Unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper text for tests to ensure passing info section.
    // Paths are also required for valid OpenAPI objects deserialization in Utoipa 5.
    const HEADER_BLOCK: &str = "info:\n  title: Test API\n  version: 1.0.0\npaths: {}";

    #[test]
    fn test_parse_simple_struct() {
        let yaml = format!(
            r#"
openapi: 3.1.0
{}
components:
  schemas:
    User:
      type: object
      properties:
        id:
          type: integer
      required: [id]
"#,
            HEADER_BLOCK
        );
        let models = parse_openapi_spec(&yaml).unwrap();
        assert_eq!(models.len(), 1);
        let ParsedModel::Struct(s) = &models[0] else {
            panic!("Expected struct")
        };
        assert_eq!(s.name, "User");
        assert_eq!(s.fields.len(), 1);
        assert_eq!(s.fields[0].name, "id");
        assert_eq!(s.fields[0].ty, "i32");
    }

    #[test]
    fn test_flatten_all_of() {
        let yaml = format!(
            r#"
openapi: 3.1.0
{}
components:
  schemas:
    Base:
      type: object
      properties:
        id: {{ type: string, format: uuid }}
    Extra:
      type: object
      properties:
        info: {{ type: string }}
    Combined:
      allOf:
        - $ref: '#/components/schemas/Base'
        - $ref: '#/components/schemas/Extra'
        - type: object
          properties:
            local: {{ type: integer }}
"#,
            HEADER_BLOCK
        );
        let models = parse_openapi_spec(&yaml).unwrap();

        let combined = models
            .iter()
            .find(|m| m.name() == "Combined")
            .expect("Combined not found");
        let ParsedModel::Struct(s) = combined else {
            panic!("Combined should be struct")
        };

        // Should have id, info, and local
        assert!(s
            .fields
            .iter()
            .any(|f| f.name == "id" && f.ty.contains("Uuid")));
        assert!(s
            .fields
            .iter()
            .any(|f| f.name == "info" && f.ty.contains("String")));
        assert!(s
            .fields
            .iter()
            .any(|f| f.name == "local" && f.ty.contains("i32")));
    }

    #[test]
    fn test_all_of_overwrite() {
        let yaml = format!(
            r#"
openapi: 3.1.0
{}
components:
  schemas:
    A:
      type: object
      properties:
        val: {{ type: integer }}
    B:
      allOf:
        - $ref: '#/components/schemas/A'
        - type: object
          properties:
            val: {{ type: string }}
"#,
            HEADER_BLOCK
        );
        let models = parse_openapi_spec(&yaml).unwrap();
        let b = models
            .iter()
            .find(|m| m.name() == "B")
            .expect("B not found");
        let ParsedModel::Struct(s) = b else {
            panic!("B should be struct")
        };

        // Check field "val"
        let val = s
            .fields
            .iter()
            .find(|f| f.name == "val")
            .expect("val missing");
        // Should be String (Optional since not required in B property definition override)
        // Original A had it optional (implicit). B has it optional.
        assert!(val.ty.contains("String"));
        assert!(!val.ty.contains("i32"));
    }

    #[test]
    fn test_one_of_extraction() {
        let yaml = format!(
            r#"
openapi: 3.1.0
{}
components:
  schemas:
    Pet:
      oneOf:
        - $ref: '#/components/schemas/Cat'
        - $ref: '#/components/schemas/Dog'
      discriminator:
        propertyName: type
"#,
            HEADER_BLOCK
        );
        let models = parse_openapi_spec(&yaml).unwrap();
        let pet = models
            .iter()
            .find(|m| m.name() == "Pet")
            .expect("Pet not found");
        let ParsedModel::Enum(e) = pet else {
            panic!("Pet should be Enum")
        };

        assert_eq!(e.tag.as_deref(), Some("type"));
        assert_eq!(e.variants.len(), 2);
        assert_eq!(e.variants[0].name, "Cat");
        assert_eq!(e.variants[0].rename, None); // No mapping implies no specific rename
    }

    #[test]
    fn test_any_of_extraction() {
        // AnyOf matches "at least one". Mapped to untagged enum.
        let yaml = format!(
            r#"
openapi: 3.1.0
{}
components:
  schemas:
    UnionType:
      anyOf:
        - $ref: '#/components/schemas/A'
        - $ref: '#/components/schemas/B'
"#,
            HEADER_BLOCK
        );
        let models = parse_openapi_spec(&yaml).unwrap();
        let union = models
            .iter()
            .find(|m| m.name() == "UnionType")
            .expect("UnionType not found");

        let ParsedModel::Enum(e) = union else {
            panic!("UnionType should be Enum")
        };
        assert!(e.untagged);
        assert!(e.tag.is_none());
        assert_eq!(e.variants.len(), 2);
        assert_eq!(e.variants[0].name, "A");
    }

    #[test]
    fn test_one_of_discriminator_mapping() {
        let yaml = format!(
            r#"
openapi: 3.1.0
{}
components:
  schemas:
    Pet:
      discriminator:
        propertyName: petType
        mapping:
          cat_variant: '#/components/schemas/Cat'
          dog_variant: '#/components/schemas/Dog'
      oneOf:
        - $ref: '#/components/schemas/Cat'
        - $ref: '#/components/schemas/Dog'
    Cat: {{type: object}}
    Dog: {{type: object}}
"#,
            HEADER_BLOCK
        );
        let models = parse_openapi_spec(&yaml).unwrap();
        let pet = models
            .iter()
            .find(|m| m.name() == "Pet")
            .expect("Pet not found");
        let ParsedModel::Enum(e) = pet else {
            panic!("Pet should be Enum")
        };

        assert_eq!(e.tag.as_deref(), Some("petType"));

        // Ensure cat_variant maps to Cat
        let cat_variant = e
            .variants
            .iter()
            .find(|v| v.name == "Cat")
            .expect("Cat variant missing");
        assert_eq!(cat_variant.rename.as_deref(), Some("cat_variant"));

        let dog_variant = e
            .variants
            .iter()
            .find(|v| v.name == "Dog")
            .expect("Dog variant missing");
        assert_eq!(dog_variant.rename.as_deref(), Some("dog_variant"));
    }
}
