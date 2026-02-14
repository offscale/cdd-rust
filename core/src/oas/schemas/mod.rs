#![deny(missing_docs)]

//! # Schema Parsing
//!
//! Handles parsing of OpenAPI `components/schemas`.
//!
//! Defines logic to:
//! - Extract struct/enum definitions.
//! - Flatten `allOf` inheritance/composition into single structs.
//! - Handle `oneOf` and `anyOf` polymorphism.
//! - Map string `enum` schemas to Rust enums.
//! - Resolve `Ref` pointers within the schema scope.
//! - **NEW**: Respects `$self` for base URI resolution (OAS 3.2 Appendix F).
//! - **NEW**: Supports schema-root documents without an OpenAPI wrapper.
//! - Propagate schema- and property-level `externalDocs` metadata.

pub mod enums;
pub mod refs;
pub mod structs;

use crate::error::{AppError, AppResult};
use crate::oas::normalization::{
    normalize_boolean_schemas, normalize_const_schemas, normalize_nullable_schemas,
};
use crate::oas::registry::DocumentRegistry;
use crate::oas::routes::shims::{ShimExternalDocs, ShimOpenApi};
use crate::oas::schemas::enums::parse_variants;
use crate::oas::schemas::refs::{
    collect_inline_schema_index, collect_schema_anchors, collect_schema_ids, compute_base_uri,
    contains_dynamic_ref, resolve_dynamic_refs_for_component, resolve_dynamic_refs_in_schema_root,
    resolve_ref_local, resolve_ref_name, ResolutionContext,
};
use crate::oas::schemas::structs::flatten_schema_fields;
use crate::oas::validation::validate_component_keys;
use crate::parser::{ParsedEnum, ParsedExternalDocs, ParsedModel, ParsedStruct, ParsedVariant};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use utoipa::openapi::schema::{AdditionalProperties, Schema, SchemaType, Type};
use utoipa::openapi::{Components, Deprecated, OpenApi, RefOr};

/// Parses a raw OpenAPI YAML string and extracts definitions.
///
/// Handles `allOf` flattening, `oneOf`/`anyOf` variants.
///
/// # Spec Compliance
/// Extracts `$self` from the root via `ShimOpenApi` to establish the document's Base URI
/// for reference resolution as per OAS 3.2.0 Appendix F.
pub fn parse_openapi_spec(yaml_content: &str) -> AppResult<Vec<ParsedModel>> {
    parse_openapi_spec_with_registry(yaml_content, None, None)
}

/// Parses an OpenAPI YAML string and extracts definitions, allowing external reference resolution.
pub fn parse_openapi_spec_with_registry(
    yaml_content: &str,
    registry: Option<&DocumentRegistry>,
    retrieval_uri: Option<&str>,
) -> AppResult<Vec<ParsedModel>> {
    // 1. Parse raw YAML/JSON to detect OpenAPI wrapper vs schema-root documents.
    let mut json_val: serde_json::Value = serde_yaml::from_str(yaml_content)
        .map_err(|e| AppError::General(format!("Failed to parse YAML container: {}", e)))?;
    let has_openapi = json_val.get("openapi").is_some() || json_val.get("swagger").is_some();
    if !has_openapi {
        normalize_nullable_schemas(&mut json_val);
        normalize_boolean_schemas(&mut json_val);
        normalize_const_schemas(&mut json_val);
        return parse_schema_root_document(&json_val);
    }

    // 2. Pre-parse to get standard fields like $self (Shim)
    let shim: ShimOpenApi = serde_yaml::from_str(yaml_content)
        .map_err(|e| AppError::General(format!("Failed to parse OpenAPI Shim: {}", e)))?;
    if let Some(components) = shim.components.as_ref() {
        validate_component_keys(components)?;
    }

    // 3. Parse using full AST (Utoipa) for schema traversal.
    // Compatibility Hack: Utoipa 5.x tightly validates the "openapi" version string.
    // If it is "3.2.0", Utoipa will panic or error.
    // We parse it into generic Value, downgrade version string to "3.1.0" for AST parsing,
    // then pass it to Utoipa.
    normalize_nullable_schemas(&mut json_val);
    normalize_boolean_schemas(&mut json_val);
    if let Some(schema_map) = json_val
        .get_mut("components")
        .and_then(|c| c.get_mut("schemas"))
        .and_then(|s| s.as_object_mut())
    {
        for schema in schema_map.values_mut() {
            normalize_const_schemas(schema);
        }
    }

    let raw_schemas = json_val
        .get("components")
        .and_then(|c| c.get("schemas"))
        .and_then(|s| s.as_object())
        .cloned();
    let (schema_docs, field_docs) = collect_schema_external_docs(raw_schemas.as_ref());
    let discriminator_defaults = collect_discriminator_defaults(raw_schemas.as_ref());
    let has_dynamic_ref = raw_schemas
        .as_ref()
        .map(|schemas| schemas.values().any(contains_dynamic_ref))
        .unwrap_or(false);

    if let Some(ver) = json_val.get_mut("openapi") {
        if let Some(raw) = ver.as_str() {
            if raw.starts_with("3.") && !raw.starts_with("3.1") {
                *ver = serde_json::json!("3.1.0");
            }
        }
    }

    let inline_source = json_val.clone();
    let openapi: OpenApi = serde_json::from_value(json_val)
        .map_err(|e| AppError::General(format!("Failed to parse OpenAPI AST: {}", e)))?;

    let components = openapi
        .components
        .as_ref()
        .ok_or_else(|| AppError::General("No components found in OpenAPI spec".into()))?;

    // 4. Initialize Resolution Context with Base URI ($self) from original Shim
    // If $self defined in Shim, use it as Base URI.
    let base_uri_str = match (retrieval_uri, shim.self_uri.as_deref()) {
        (Some(retrieval), Some(self_val)) => Some(compute_base_uri(retrieval, Some(self_val))),
        (Some(retrieval), None) => Some(retrieval.to_string()),
        (None, Some(self_val)) => Some(self_val.to_string()),
        (None, None) => None,
    };
    let mut ctx = ResolutionContext::with_registry(base_uri_str, components, registry);
    ctx.schema_ids = collect_schema_ids(raw_schemas.as_ref(), ctx.base_uri.as_ref());
    ctx.schema_anchors = collect_schema_anchors(raw_schemas.as_ref(), ctx.base_uri.as_ref());
    let inline_index = collect_inline_schema_index(&inline_source, ctx.base_uri.as_ref(), true);
    ctx.inline_schema_ids = inline_index.ids;
    ctx.inline_schema_anchors = inline_index.anchors;

    let mut models = Vec::new();

    {
        let mut handle_schema = |schema_name: &str,
                                 schema: &Schema,
                                 ctx_ref: &ResolutionContext<'_>|
         -> AppResult<()> {
            match schema {
                // Case 1: Simple Object or AllOf (Merged Struct)
                Schema::Object(obj) => {
                    if let Some(variants) = extract_string_enum_variants(obj) {
                        models.push(ParsedModel::Enum(ParsedEnum {
                            name: schema_name.to_string(),
                            description: obj.description.clone(),
                            rename: None,
                            rename_all: None,
                            tag: None,
                            untagged: false,
                            variants,
                            is_deprecated: matches!(obj.deprecated, Some(Deprecated::True)),
                            external_docs: schema_docs.get(schema_name).cloned(),
                            discriminator_mapping: None,
                            discriminator_default_mapping: None,
                        }));
                        return Ok(());
                    }

                    // Pass Context to flattener
                    let mut parsed_fields = flatten_schema_fields(schema, ctx_ref)?;
                    apply_field_external_docs(&mut parsed_fields, field_docs.get(schema_name));

                    // Extract description and metadata if present on the top level
                    // Note: utoipa::openapi::Object missing external_docs access
                    let (description, deprecated) = (
                        obj.description.clone(),
                        matches!(obj.deprecated, Some(Deprecated::True)),
                    );

                    let deny_unknown_fields = schema_denies_unknown_fields(schema, ctx_ref);
                    models.push(ParsedModel::Struct(ParsedStruct {
                        name: schema_name.to_string(),
                        description,
                        rename: None,
                        rename_all: None,
                        fields: parsed_fields,
                        is_deprecated: deprecated,
                        deny_unknown_fields,
                        external_docs: schema_docs.get(schema_name).cloned(),
                    }));
                }
                Schema::AllOf(_) => {
                    // Pass Context to flattener
                    let mut parsed_fields = flatten_schema_fields(schema, ctx_ref)?;
                    apply_field_external_docs(&mut parsed_fields, field_docs.get(schema_name));

                    // Extract description and metadata if present on the top level
                    // Note: utoipa::openapi::Object missing external_docs access
                    let (description, deprecated) = match schema {
                        Schema::AllOf(a) => (a.description.clone(), false),
                        _ => (None, false),
                    };

                    let deny_unknown_fields = schema_denies_unknown_fields(schema, ctx_ref);
                    models.push(ParsedModel::Struct(ParsedStruct {
                        name: schema_name.to_string(),
                        description,
                        rename: None,
                        rename_all: None,
                        fields: parsed_fields,
                        is_deprecated: deprecated,
                        deny_unknown_fields,
                        external_docs: schema_docs.get(schema_name).cloned(),
                    }));
                }
                // Case 2: OneOf (Enum)
                Schema::OneOf(one_of) => {
                    if let Some(raw_schema) = raw_schemas
                        .as_ref()
                        .and_then(|schemas| schemas.get(schema_name))
                    {
                        if let Some(variants) = extract_const_enum_variants(raw_schema) {
                            models.push(ParsedModel::Enum(ParsedEnum {
                                name: schema_name.to_string(),
                                description: raw_schema
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                rename: None,
                                rename_all: None,
                                tag: None,
                                untagged: false,
                                variants,
                                is_deprecated: false,
                                external_docs: schema_docs.get(schema_name).cloned(),
                                discriminator_mapping: None,
                                discriminator_default_mapping: None,
                            }));
                            return Ok(());
                        }
                    }

                    let discriminator = one_of.discriminator.clone();
                    let tag = discriminator.as_ref().map(|d| d.property_name.clone());
                    // mapping is a BTreeMap, we access it directly via reference mapping
                    let mapping = discriminator.as_ref().map(|d| &d.mapping);
                    let is_untagged = tag.is_none();

                    let variants = parse_variants(&one_of.items, mapping);

                    // Capture formatting of mapping for documentation
                    let mapping_cloned = mapping.cloned();
                    let default_mapping = discriminator_defaults.get(schema_name).cloned();

                    if !variants.is_empty() {
                        models.push(ParsedModel::Enum(ParsedEnum {
                            name: schema_name.to_string(),
                            description: None,
                            rename: None,
                            rename_all: None,
                            tag,
                            untagged: is_untagged,
                            variants,
                            is_deprecated: false,
                            external_docs: schema_docs.get(schema_name).cloned(),
                            discriminator_mapping: mapping_cloned,
                            discriminator_default_mapping: default_mapping,
                        }));
                    }
                }
                // Case 3: AnyOf (Enum)
                // Treated similarly to OneOf: untagged enum which validates "matches at least one".
                // In Rust serde "untagged", this creates an enum that tries variants in order.
                Schema::AnyOf(any_of) => {
                    if let Some(raw_schema) = raw_schemas
                        .as_ref()
                        .and_then(|schemas| schemas.get(schema_name))
                    {
                        if let Some(variants) = extract_const_enum_variants(raw_schema) {
                            models.push(ParsedModel::Enum(ParsedEnum {
                                name: schema_name.to_string(),
                                description: raw_schema
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                rename: None,
                                rename_all: None,
                                tag: None,
                                untagged: false,
                                variants,
                                is_deprecated: false,
                                external_docs: schema_docs.get(schema_name).cloned(),
                                discriminator_mapping: None,
                                discriminator_default_mapping: None,
                            }));
                            return Ok(());
                        }
                    }

                    // AnyOf also supports discriminator in OAS 3.x, though less common.
                    let discriminator = any_of.discriminator.clone();
                    let tag = discriminator.as_ref().map(|d| d.property_name.clone());
                    let mapping = discriminator.as_ref().map(|d| &d.mapping);

                    // Default anyOf to untagged unless strict discriminator is used
                    let is_untagged = tag.is_none();

                    let variants = parse_variants(&any_of.items, mapping);
                    let mapping_cloned = mapping.cloned();
                    let default_mapping = discriminator_defaults.get(schema_name).cloned();

                    if !variants.is_empty() {
                        models.push(ParsedModel::Enum(ParsedEnum {
                            name: schema_name.to_string(),
                            description: Some("AnyOf: Matches at least one schema".to_string()),
                            rename: None,
                            rename_all: None,
                            tag,
                            untagged: is_untagged,
                            variants,
                            is_deprecated: false,
                            external_docs: schema_docs.get(schema_name).cloned(),
                            discriminator_mapping: mapping_cloned,
                            discriminator_default_mapping: default_mapping,
                        }));
                    }
                }
                // Note: `not` schema is not currently supported by utoipa::openapi::Schema enum parsing.
                // If supported in future updates, we would map it to a generic Value wrapper.
                _ => {}
            }
            Ok(())
        };

        for (name, ref_or_schema) in &components.schemas {
            if has_dynamic_ref {
                if let Some(raw_map) = raw_schemas.as_ref() {
                    let resolved_map = resolve_dynamic_refs_for_component(
                        name,
                        raw_map,
                        &ctx.schema_ids,
                        ctx.base_uri.as_ref(),
                        shim.self_uri.as_deref(),
                    );
                    if !resolved_map.is_empty() {
                        let mut comps = components.clone();
                        apply_resolved_schemas(&mut comps, &resolved_map)?;
                        let local_ctx = ResolutionContext {
                            base_uri: ctx.base_uri.clone(),
                            components: &comps,
                            schema_ids: ctx.schema_ids.clone(),
                            schema_anchors: ctx.schema_anchors.clone(),
                            inline_schema_ids: ctx.inline_schema_ids.clone(),
                            inline_schema_anchors: ctx.inline_schema_anchors.clone(),
                            registry: ctx.registry,
                        };
                        if let Some(schema) = comps
                            .schemas
                            .get(name)
                            .and_then(|ref_or| resolve_ref_local(ref_or, &local_ctx))
                        {
                            handle_schema(name, schema, &local_ctx)?;
                        }
                        continue;
                    }
                }
            }

            let schema = resolve_ref_local(ref_or_schema, &ctx);
            if let Some(s) = schema {
                handle_schema(name, s, &ctx)?;
            }
        }
    }

    Ok(models)
}

fn parse_schema_root_document(raw: &serde_json::Value) -> AppResult<Vec<ParsedModel>> {
    let resolved_raw = if contains_dynamic_ref(raw) {
        resolve_dynamic_refs_in_schema_root(raw, None)
    } else {
        raw.clone()
    };

    let name = schema_root_name(&resolved_raw);
    let schema: Schema = serde_json::from_value(resolved_raw.clone())
        .map_err(|e| AppError::General(format!("Failed to parse schema-root document: {}", e)))?;

    let mut map = serde_json::Map::new();
    map.insert(name.clone(), resolved_raw.clone());
    let (schema_docs, field_docs) = collect_schema_external_docs(Some(&map));

    let components = Components::new();
    let mut ctx = ResolutionContext::new(None, &components);
    let inline_index = collect_inline_schema_index(raw, ctx.base_uri.as_ref(), false);
    ctx.inline_schema_ids = inline_index.ids;
    ctx.inline_schema_anchors = inline_index.anchors;

    let mut models = Vec::new();
    match &schema {
        Schema::Object(obj) => {
            if let Some(variants) = extract_string_enum_variants(obj) {
                models.push(ParsedModel::Enum(ParsedEnum {
                    name: name.clone(),
                    description: obj.description.clone(),
                    rename: None,
                    rename_all: None,
                    tag: None,
                    untagged: false,
                    variants,
                    is_deprecated: matches!(obj.deprecated, Some(Deprecated::True)),
                    external_docs: schema_docs.get(&name).cloned(),
                    discriminator_mapping: None,
                    discriminator_default_mapping: None,
                }));
                return Ok(models);
            }

            let mut parsed_fields = flatten_schema_fields(&schema, &ctx)?;
            apply_field_external_docs(&mut parsed_fields, field_docs.get(&name));

            let deny_unknown_fields = schema_denies_unknown_fields(&schema, &ctx);
            models.push(ParsedModel::Struct(ParsedStruct {
                name: name.clone(),
                description: obj.description.clone(),
                rename: None,
                rename_all: None,
                fields: parsed_fields,
                is_deprecated: matches!(obj.deprecated, Some(Deprecated::True)),
                deny_unknown_fields,
                external_docs: schema_docs.get(&name).cloned(),
            }));
        }
        Schema::AllOf(all_of) => {
            let mut parsed_fields = flatten_schema_fields(&schema, &ctx)?;
            apply_field_external_docs(&mut parsed_fields, field_docs.get(&name));

            let deny_unknown_fields = schema_denies_unknown_fields(&schema, &ctx);
            models.push(ParsedModel::Struct(ParsedStruct {
                name: name.clone(),
                description: all_of.description.clone(),
                rename: None,
                rename_all: None,
                fields: parsed_fields,
                is_deprecated: false,
                deny_unknown_fields,
                external_docs: schema_docs.get(&name).cloned(),
            }));
        }
        Schema::OneOf(one_of) => {
            if let Some(variants) = extract_const_enum_variants(raw) {
                models.push(ParsedModel::Enum(ParsedEnum {
                    name: name.clone(),
                    description: raw
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    rename: None,
                    rename_all: None,
                    tag: None,
                    untagged: false,
                    variants,
                    is_deprecated: false,
                    external_docs: schema_docs.get(&name).cloned(),
                    discriminator_mapping: None,
                    discriminator_default_mapping: None,
                }));
                return Ok(models);
            }

            let discriminator = one_of.discriminator.clone();
            let tag = discriminator.as_ref().map(|d| d.property_name.clone());
            let mapping = discriminator.as_ref().map(|d| &d.mapping);
            let is_untagged = tag.is_none();
            let variants = parse_variants(&one_of.items, mapping);

            if !variants.is_empty() {
                models.push(ParsedModel::Enum(ParsedEnum {
                    name: name.clone(),
                    description: None,
                    rename: None,
                    rename_all: None,
                    tag,
                    untagged: is_untagged,
                    variants,
                    is_deprecated: false,
                    external_docs: schema_docs.get(&name).cloned(),
                    discriminator_mapping: mapping.cloned(),
                    discriminator_default_mapping: None,
                }));
            }
        }
        Schema::AnyOf(any_of) => {
            let discriminator = any_of.discriminator.clone();
            let tag = discriminator.as_ref().map(|d| d.property_name.clone());
            let mapping = discriminator.as_ref().map(|d| &d.mapping);
            let is_untagged = tag.is_none();
            let variants = parse_variants(&any_of.items, mapping);

            if !variants.is_empty() {
                models.push(ParsedModel::Enum(ParsedEnum {
                    name: name.clone(),
                    description: None,
                    rename: None,
                    rename_all: None,
                    tag,
                    untagged: is_untagged,
                    variants,
                    is_deprecated: false,
                    external_docs: schema_docs.get(&name).cloned(),
                    discriminator_mapping: mapping.cloned(),
                    discriminator_default_mapping: None,
                }));
            }
        }
        _ => {
            return Err(AppError::General(format!(
                "Schema-root document '{}' must be an object or oneOf/anyOf schema",
                name
            )));
        }
    }

    Ok(models)
}

fn apply_resolved_schemas(
    components: &mut Components,
    resolved: &HashMap<String, Value>,
) -> AppResult<()> {
    for (name, value) in resolved {
        let mut normalized = value.clone();
        normalize_nullable_schemas(&mut normalized);
        normalize_boolean_schemas(&mut normalized);
        normalize_const_schemas(&mut normalized);
        let ref_or: RefOr<Schema> = serde_json::from_value(normalized).map_err(|e| {
            AppError::General(format!("Failed to parse resolved schema '{}': {}", name, e))
        })?;
        components.schemas.insert(name.clone(), ref_or);
    }
    Ok(())
}

fn schema_root_name(raw: &serde_json::Value) -> String {
    if let Some(title) = raw
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return sanitize_schema_name(title);
    }

    if let Some(id) = raw
        .get("$id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return sanitize_schema_name(&schema_name_from_id(id));
    }

    "RootSchema".to_string()
}

fn schema_name_from_id(raw: &str) -> String {
    let trimmed = raw.split('#').next().unwrap_or(raw).trim_end_matches('/');
    let last = trimmed
        .rsplit('/')
        .next()
        .unwrap_or(trimmed)
        .rsplit('\\')
        .next()
        .unwrap_or(trimmed);
    let stem = last.split('.').next().unwrap_or(last);
    if stem.is_empty() {
        "RootSchema".to_string()
    } else {
        stem.to_string()
    }
}

fn sanitize_schema_name(raw: &str) -> String {
    let mut out = String::new();
    let mut capitalize_next = true;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            if capitalize_next {
                out.push(ch.to_ascii_uppercase());
            } else {
                out.push(ch);
            }
            capitalize_next = false;
        } else {
            capitalize_next = true;
        }
    }

    if out.is_empty() {
        "RootSchema".to_string()
    } else if out
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        format!("Schema{}", out)
    } else {
        out
    }
}

/// Collects ExternalDocs metadata from raw component schema JSON.
///
/// Returns two maps:
/// - schema-level externalDocs by schema name
/// - property-level externalDocs by schema name, then property name
fn collect_schema_external_docs(
    raw_schemas: Option<&serde_json::Map<String, Value>>,
) -> (
    HashMap<String, ParsedExternalDocs>,
    HashMap<String, HashMap<String, ParsedExternalDocs>>,
) {
    let mut schema_docs = HashMap::new();
    let mut field_docs = HashMap::new();

    let Some(schemas) = raw_schemas else {
        return (schema_docs, field_docs);
    };

    for (schema_name, schema_val) in schemas {
        if let Some(ext_val) = schema_val.get("externalDocs") {
            if let Some(doc) = parse_external_docs_value(ext_val) {
                schema_docs.insert(schema_name.clone(), doc);
            }
        }

        if let Some(props) = schema_val.get("properties").and_then(|p| p.as_object()) {
            let mut per_field = HashMap::new();
            for (prop_name, prop_val) in props {
                if let Some(ext_val) = prop_val.get("externalDocs") {
                    if let Some(doc) = parse_external_docs_value(ext_val) {
                        per_field.insert(prop_name.clone(), doc);
                    }
                }
            }
            if !per_field.is_empty() {
                field_docs.insert(schema_name.clone(), per_field);
            }
        }
    }

    (schema_docs, field_docs)
}

fn schema_denies_unknown_fields(schema: &Schema, ctx: &ResolutionContext) -> bool {
    match schema {
        Schema::Object(obj) => {
            if let Some(props) = &obj.additional_properties {
                matches!(**props, AdditionalProperties::FreeForm(false))
            } else {
                false
            }
        }
        Schema::AllOf(all_of) => all_of
            .items
            .iter()
            .any(|item| ref_or_denies_unknown_fields(item, ctx)),
        _ => false,
    }
}

fn ref_or_denies_unknown_fields(item: &RefOr<Schema>, ctx: &ResolutionContext) -> bool {
    match item {
        RefOr::T(schema) => schema_denies_unknown_fields(schema, ctx),
        RefOr::Ref(r) => resolve_ref_name(&r.ref_location, ctx)
            .map(|schema| schema_denies_unknown_fields(schema, ctx))
            .unwrap_or(false),
    }
}

fn collect_discriminator_defaults(
    raw_schemas: Option<&serde_json::Map<String, Value>>,
) -> HashMap<String, String> {
    let mut defaults = HashMap::new();
    let Some(schemas) = raw_schemas else {
        return defaults;
    };

    for (schema_name, schema_val) in schemas {
        let Some(discriminator) = schema_val.get("discriminator") else {
            continue;
        };
        let Some(default_mapping) = discriminator.get("defaultMapping").and_then(|v| v.as_str())
        else {
            continue;
        };
        if !default_mapping.trim().is_empty() {
            defaults.insert(schema_name.clone(), default_mapping.to_string());
        }
    }

    defaults
}

fn parse_external_docs_value(value: &Value) -> Option<ParsedExternalDocs> {
    serde_json::from_value::<ShimExternalDocs>(value.clone())
        .ok()
        .map(|doc| ParsedExternalDocs {
            url: doc.url,
            description: doc.description,
        })
}

fn apply_field_external_docs(
    fields: &mut [crate::parser::ParsedField],
    docs: Option<&HashMap<String, ParsedExternalDocs>>,
) {
    let Some(map) = docs else {
        return;
    };

    for field in fields {
        if let Some(doc) = map.get(&field.name) {
            field.external_docs = Some(doc.clone());
        }
    }
}

fn extract_string_enum_variants(
    obj: &utoipa::openapi::schema::Object,
) -> Option<Vec<ParsedVariant>> {
    if !matches!(obj.schema_type, SchemaType::Type(Type::String)) {
        return None;
    }

    let enum_values = obj.enum_values.as_ref()?;
    let mut variants = Vec::new();
    let mut seen = HashSet::new();

    for value in enum_values.iter() {
        let Some(raw) = value.as_str() else {
            return None;
        };

        let base = sanitize_enum_variant(raw);
        let mut name = base.clone();
        let mut suffix = 1;
        while !seen.insert(name.clone()) {
            suffix += 1;
            name = format!("{}{}", base, suffix);
        }

        variants.push(ParsedVariant {
            name,
            ty: None,
            description: None,
            rename: Some(raw.to_string()),
            aliases: Some(Vec::new()),
            is_deprecated: matches!(obj.deprecated, Some(Deprecated::True)),
        });
    }

    Some(variants)
}

fn extract_const_enum_variants(raw_schema: &Value) -> Option<Vec<ParsedVariant>> {
    let obj = raw_schema.as_object()?;
    let items = obj.get("oneOf").or_else(|| obj.get("anyOf"))?.as_array()?;

    if items.is_empty() {
        return None;
    }

    let mut variants = Vec::new();
    let mut seen = HashSet::new();

    for item in items {
        let item_obj = item.as_object()?;
        let const_val = item_obj.get("const").or_else(|| {
            item_obj
                .get("enum")
                .and_then(|v| v.as_array())
                .and_then(|vals| (vals.len() == 1).then_some(&vals[0]))
        })?;
        let Some(const_str) = const_val.as_str() else {
            return None;
        };

        let base_name = item_obj
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(const_str);
        let base = sanitize_enum_variant(base_name);
        let mut name = base.clone();
        let mut suffix = 1;
        while !seen.insert(name.clone()) {
            suffix += 1;
            name = format!("{}{}", base, suffix);
        }

        let description = item_obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let deprecated = item_obj
            .get("deprecated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        variants.push(ParsedVariant {
            name,
            ty: None,
            description,
            rename: Some(const_str.to_string()),
            aliases: Some(Vec::new()),
            is_deprecated: deprecated,
        });
    }

    Some(variants)
}

fn sanitize_enum_variant(value: &str) -> String {
    let mut out = String::new();
    let mut capitalize = true;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if capitalize {
                out.push(ch.to_ascii_uppercase());
                capitalize = false;
            } else {
                out.push(ch);
            }
        } else {
            capitalize = true;
        }
    }

    if out.is_empty() {
        out.push_str("Value");
    }

    if out
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        out = format!("Value{}", out);
    }

    if is_rust_keyword(&out) {
        out.push_str("Value");
    }

    out
}

fn is_rust_keyword(ident: &str) -> bool {
    matches!(
        ident,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
    )
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

    #[test]
    fn test_parse_string_enum_schema() {
        let yaml = format!(
            r#" 
openapi: 3.1.0
{} 
components: 
  schemas: 
    Status: 
      type: string
      enum: [active, "on-hold"] 
"#,
            HEADER_BLOCK
        );

        let models = parse_openapi_spec(&yaml).unwrap();
        let status = models
            .iter()
            .find(|m| m.name() == "Status")
            .expect("Status enum missing");

        let ParsedModel::Enum(en) = status else {
            panic!("Status should be parsed as enum")
        };

        assert_eq!(en.variants.len(), 2);
        assert_eq!(en.variants[0].rename.as_deref(), Some("active"));
        assert!(en.variants.iter().any(|v| v.name == "OnHold"));
    }

    #[test]
    fn test_nullable_property_maps_to_option() {
        let yaml = format!(
            r#" 
openapi: 3.0.0
{} 
components: 
  schemas: 
    User: 
      type: object
      required: [name]
      properties:
        name:
          type: string
          x-nullable: true
"#,
            HEADER_BLOCK
        );

        let models = parse_openapi_spec(&yaml).unwrap();
        let user = models
            .iter()
            .find(|m| m.name() == "User")
            .expect("User schema missing");

        let ParsedModel::Struct(s) = user else {
            panic!("User should be parsed as struct");
        };

        let name_field = s.fields.iter().find(|f| f.name == "name").unwrap();
        assert_eq!(name_field.ty, "Option<String>");
    }

    #[test]
    fn test_parse_const_enum_oneof() {
        let yaml = format!(
            r#"
openapi: 3.2.0
{}
components:
  schemas:
    ColorSpace:
      oneOf:
        - const: RGB
          title: RGB
        - const: CMYK
          description: Subtractive
"#,
            HEADER_BLOCK
        );

        let models = parse_openapi_spec(&yaml).unwrap();
        let color_space = models
            .iter()
            .find(|m| m.name() == "ColorSpace")
            .expect("ColorSpace enum missing");

        let ParsedModel::Enum(en) = color_space else {
            panic!("ColorSpace should be parsed as enum");
        };

        assert_eq!(en.variants.len(), 2);
        assert!(en
            .variants
            .iter()
            .any(|v| v.rename.as_deref() == Some("RGB")));
        assert!(en
            .variants
            .iter()
            .any(|v| v.rename.as_deref() == Some("CMYK")));
    }

    #[test]
    fn test_parse_discriminator_default_mapping() {
        let yaml = format!(
            r#" 
openapi: 3.2.0
{} 
components: 
  schemas: 
    Pet: 
      oneOf: 
        - $ref: '#/components/schemas/Cat' 
        - $ref: '#/components/schemas/Dog' 
      discriminator: 
        propertyName: petType
        defaultMapping: '#/components/schemas/OtherPet' 
    Cat: 
      type: object
    Dog: 
      type: object
    OtherPet: 
      type: object
"#,
            HEADER_BLOCK
        );

        let models = parse_openapi_spec(&yaml).unwrap();
        let pet = models
            .iter()
            .find(|m| m.name() == "Pet")
            .expect("Pet enum missing");

        let ParsedModel::Enum(en) = pet else {
            panic!("Pet should be parsed as enum")
        };

        assert_eq!(
            en.discriminator_default_mapping.as_deref(),
            Some("#/components/schemas/OtherPet")
        );
    }

    #[test]
    fn test_external_schema_ref_with_registry() {
        let external = r#"
$id: https://example.com/schemas/Shared.json
type: object
properties:
  name:
    type: string
"#;
        let mut registry = DocumentRegistry::new();
        registry
            .register_schema_yaml("https://example.com/schemas/Shared.json", external)
            .unwrap();

        let yaml = format!(
            r#"
openapi: 3.2.0
{}
components:
  schemas:
    Shared:
      $ref: https://example.com/schemas/Shared.json
"#,
            HEADER_BLOCK
        );

        let models = parse_openapi_spec_with_registry(&yaml, Some(&registry), None).unwrap();
        let shared = models.iter().find(|m| m.name() == "Shared").unwrap();
        let ParsedModel::Struct(s) = shared else {
            panic!("Shared should be a struct")
        };
        assert!(s.fields.iter().any(|f| f.name == "name"));
    }

    #[test]
    fn test_schema_external_docs_propagation() {
        let yaml = format!(
            r#" 
openapi: 3.1.0
{} 
components: 
  schemas: 
    User: 
      type: object
      externalDocs: 
        url: https://example.com/user
        description: User docs
      properties: 
        id: 
          type: string
          externalDocs: 
            url: https://example.com/user/id
            description: Id docs
"#,
            HEADER_BLOCK
        );

        let models = parse_openapi_spec(&yaml).unwrap();
        let user = models
            .iter()
            .find(|m| m.name() == "User")
            .expect("User schema missing");

        let ParsedModel::Struct(s) = user else {
            panic!("User should be parsed as struct")
        };

        let docs = s.external_docs.as_ref().expect("schema docs missing");
        assert_eq!(docs.url, "https://example.com/user");
        assert_eq!(docs.description.as_deref(), Some("User docs"));

        let id_field = s.fields.iter().find(|f| f.name == "id").unwrap();
        let id_docs = id_field.external_docs.as_ref().expect("field docs missing");
        assert_eq!(id_docs.url, "https://example.com/user/id");
        assert_eq!(id_docs.description.as_deref(), Some("Id docs"));
    }

    #[test]
    fn test_additional_properties_false_sets_deny_unknown_fields() {
        let yaml = format!(
            r#" 
openapi: 3.1.0
{} 
components: 
  schemas: 
    User: 
      type: object
      additionalProperties: false
      properties: 
        id: 
          type: string
"#,
            HEADER_BLOCK
        );

        let models = parse_openapi_spec(&yaml).unwrap();
        let user = models
            .iter()
            .find(|m| m.name() == "User")
            .expect("User schema missing");
        let ParsedModel::Struct(s) = user else {
            panic!("User should be parsed as struct")
        };

        assert!(s.deny_unknown_fields);
        assert!(s.fields.iter().all(|f| f.name != "additional_properties"));
    }

    #[test]
    fn test_all_of_additional_properties_false_sets_deny_unknown_fields() {
        let yaml = format!(
            r#" 
openapi: 3.1.0
{} 
components: 
  schemas: 
    Base: 
      type: object
      additionalProperties: false
      properties: 
        id: 
          type: string
    Extra: 
      type: object
      properties: 
        note: 
          type: string
    Combined: 
      allOf: 
        - $ref: '#/components/schemas/Base' 
        - $ref: '#/components/schemas/Extra' 
"#,
            HEADER_BLOCK
        );

        let models = parse_openapi_spec(&yaml).unwrap();
        let combined = models
            .iter()
            .find(|m| m.name() == "Combined")
            .expect("Combined schema missing");
        let ParsedModel::Struct(s) = combined else {
            panic!("Combined should be parsed as struct")
        };

        assert!(s.deny_unknown_fields);
        assert!(s.fields.iter().all(|f| f.name != "additional_properties"));
    }

    #[test]
    fn test_parse_schema_root_with_title() {
        let yaml = r#"
title: User
type: object
properties:
  id:
    type: integer
"#;

        let models = parse_openapi_spec(yaml).unwrap();
        assert_eq!(models.len(), 1);
        let ParsedModel::Struct(s) = &models[0] else {
            panic!("Schema root should parse as struct")
        };
        assert_eq!(s.name, "User");
        assert_eq!(s.fields.len(), 1);
        assert_eq!(s.fields[0].name, "id");
    }

    #[test]
    fn test_parse_schema_root_with_id_fallback() {
        let yaml = r#"
$id: https://example.com/schemas/Pet.json
type: object
properties:
  name:
    type: string
"#;

        let models = parse_openapi_spec(yaml).unwrap();
        assert_eq!(models.len(), 1);
        let ParsedModel::Struct(s) = &models[0] else {
            panic!("Schema root should parse as struct")
        };
        assert_eq!(s.name, "Pet");
        assert_eq!(s.fields.len(), 1);
        assert_eq!(s.fields[0].name, "name");
    }
}
