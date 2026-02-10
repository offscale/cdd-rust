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
//! - **NEW**: Respects `$self` for base URI resolution (OAS 3.2 Appendix F).

pub mod enums;
pub mod refs;
pub mod structs;

use crate::error::{AppError, AppResult};
use crate::oas::routes::shims::ShimOpenApi;
use crate::oas::schemas::enums::parse_variants;
use crate::oas::schemas::refs::{resolve_ref_local, ResolutionContext};
use crate::oas::schemas::structs::flatten_schema_fields;
use crate::oas::validation::validate_component_keys;
use crate::parser::{ParsedEnum, ParsedModel, ParsedStruct};
use utoipa::openapi::{schema::Schema, Deprecated, OpenApi};

/// Parses a raw OpenAPI YAML string and extracts definitions.
///
/// Handles `allOf` flattening, `oneOf`/`anyOf` variants.
///
/// # Spec Compliance
/// Extracts `$self` from the root via `ShimOpenApi` to establish the document's Base URI
/// for reference resolution as per OAS 3.2.0 Appendix F.
pub fn parse_openapi_spec(yaml_content: &str) -> AppResult<Vec<ParsedModel>> {
    // 1. Pre-parse to get standard fields like $self (Shim)
    let shim: ShimOpenApi = serde_yaml::from_str(yaml_content)
        .map_err(|e| AppError::General(format!("Failed to parse OpenAPI Shim: {}", e)))?;
    if let Some(components) = shim.components.as_ref() {
        validate_component_keys(components)?;
    }

    // 2. Parse using full AST (Utoipa) for schema traversal.
    // Compatibility Hack: Utoipa 5.x tightly validates the "openapi" version string.
    // If it is "3.2.0", Utoipa will panic or error.
    // We parse it into generic Value, downgrade version string to "3.1.0" for AST parsing,
    // then pass it to Utoipa.
    let mut json_val: serde_json::Value = serde_yaml::from_str(yaml_content)
        .map_err(|e| AppError::General(format!("Failed to parse YAML container: {}", e)))?;

    if let Some(ver) = json_val.get_mut("openapi") {
        if ver.as_str() == Some("3.2.0") {
            *ver = serde_json::json!("3.1.0");
        }
    }

    let openapi: OpenApi = serde_json::from_value(json_val)
        .map_err(|e| AppError::General(format!("Failed to parse OpenAPI AST: {}", e)))?;

    let components = openapi
        .components
        .as_ref()
        .ok_or_else(|| AppError::General("No components found in OpenAPI spec".into()))?;

    // 3. Initialize Resolution Context with Base URI ($self) from original Shim
    // If $self defined in Shim, use it as Base URI.
    let ctx = ResolutionContext::new(shim.self_uri, components);

    let mut models = Vec::new();

    {
        for (name, ref_or_schema) in &components.schemas {
            // Resolve local ref if usually top-level
            let schema = resolve_ref_local(ref_or_schema, &ctx);

            if let Some(s) = schema {
                match s {
                    // Case 1: Simple Object or AllOf (Merged Struct)
                    Schema::Object(_) | Schema::AllOf(_) => {
                        // Pass Context to flattener
                        let parsed_fields = flatten_schema_fields(s, &ctx)?;

                        // Extract description and metadata if present on the top level
                        // Note: utoipa::openapi::Object missing external_docs access
                        let (description, deprecated) = match s {
                            Schema::Object(o) => (
                                o.description.clone(),
                                matches!(o.deprecated, Some(Deprecated::True)),
                            ),
                            Schema::AllOf(a) => (a.description.clone(), false),
                            _ => (None, false),
                        };

                        models.push(ParsedModel::Struct(ParsedStruct {
                            name: name.clone(),
                            description,
                            rename: None,
                            fields: parsed_fields,
                            is_deprecated: deprecated,
                            external_docs: None,
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

                        // Capture formatting of mapping for documentation
                        let mapping_cloned = mapping.cloned();

                        if !variants.is_empty() {
                            models.push(ParsedModel::Enum(ParsedEnum {
                                name: name.clone(),
                                description: None,
                                rename: None,
                                tag,
                                untagged: is_untagged,
                                variants,
                                is_deprecated: false,
                                external_docs: None,
                                discriminator_mapping: mapping_cloned,
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
                        let mapping_cloned = mapping.cloned();

                        if !variants.is_empty() {
                            models.push(ParsedModel::Enum(ParsedEnum {
                                name: name.clone(),
                                description: Some("AnyOf: Matches at least one schema".to_string()),
                                rename: None,
                                tag,
                                untagged: is_untagged,
                                variants,
                                is_deprecated: false,
                                external_docs: None,
                                discriminator_mapping: mapping_cloned,
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

#[cfg(test)]
mod tests {
    use super::*;

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

        assert!(s.fields.iter().any(|f| f.name == "id"));
        assert!(s.fields.iter().any(|f| f.name == "info"));
        assert!(s.fields.iter().any(|f| f.name == "local"));
    }

    #[test]
    fn test_self_reference_resolution_appendix_f() {
        // This test ensures that when $self is present, absolute references matching it
        // (with fragments) are resolved as local references.
        // It also checks that OAS 3.2.0 version string is shimmmed correctly via the parser update.
        let yaml = format!(
            r#"
openapi: 3.2.0
$self: https://my-api.com/v1/spec.yaml
{}
components:
  schemas:
    Local:
      type: object
      properties:
        val: {{ type: integer }}
    RemoteRef:
      type: object
      properties:
        # This is an absolute URI matching $self, should resolve to Local
        field:
          $ref: 'https://my-api.com/v1/spec.yaml#/components/schemas/Local'
"#,
            HEADER_BLOCK
        );

        let models = parse_openapi_spec(&yaml).unwrap();

        let remote = models
            .iter()
            .find(|m| m.name() == "RemoteRef")
            .expect("RemoteRef found");
        let ParsedModel::Struct(s) = remote else {
            panic!("Struct expected")
        };

        let target_field = s.fields.iter().find(|f| f.name == "field").unwrap();
        // Our map_schema_to_rust_type splits by `/` and takes last.
        assert_eq!(target_field.ty, "Option<Local>");
    }
}
