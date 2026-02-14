#![deny(missing_docs)]

//! # Parameter Resolution
//!
//! Logic for resolving OpenAPI Parameters into internal `RouteParam` structs.
//! Handles styles, explode, legacy Swagger 2.0 compatibility, and
//! type-aware style validation for OAS 3.x parameters.

use crate::error::{AppError, AppResult};
use crate::oas::models::{ContentMediaType, ExampleValue, ParamSource, ParamStyle, RouteParam};
use crate::oas::ref_utils::extract_component_name;
use crate::oas::registry::DocumentRegistry;
use crate::oas::resolver::types::{map_schema_to_rust_type, map_schema_to_rust_type_with_raw};
use crate::oas::routes::shims::ShimComponents;
use crate::oas::validation::validate_example_object_value;
use serde::de::Error as DeError;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{json, Value as JsonValue};
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use url::Url;
use utoipa::openapi::content::Content;
use utoipa::openapi::example::Example;
use utoipa::openapi::schema::{Schema, SchemaType, Type};
use utoipa::openapi::RefOr;

/// Wrapper for schema values that can be either a Schema Object or a boolean.
///
/// OpenAPI 3.1+ allows `schema: true/false` in places where Schema Objects are accepted.
/// We preserve the boolean so we can apply correct parameter/header semantics.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SchemaOrBool {
    /// A standard Schema Object (or $ref).
    Schema(RefOr<Schema>),
    /// Boolean Schema (`true` allows any instance, `false` allows none).
    Bool(bool),
}

impl SchemaOrBool {
    fn as_schema(&self) -> Option<&RefOr<Schema>> {
        match self {
            SchemaOrBool::Schema(schema) => Some(schema),
            SchemaOrBool::Bool(_) => None,
        }
    }

    fn is_false(&self) -> bool {
        matches!(self, SchemaOrBool::Bool(false))
    }
}

impl From<RefOr<Schema>> for SchemaOrBool {
    fn from(value: RefOr<Schema>) -> Self {
        SchemaOrBool::Schema(value)
    }
}

impl From<Schema> for SchemaOrBool {
    fn from(value: Schema) -> Self {
        SchemaOrBool::Schema(RefOr::T(value))
    }
}

/// A local shim for Parameter to ensure robust parsing of fields.
/// Includes OAS 3.x style fields and OAS 2.0 `collectionFormat`.
///
/// Note: we retain the raw JSON to preserve `content`-level OAS 3.2 fields
/// (e.g. `itemSchema`, `serializedValue`) that are not modeled by `utoipa`.
#[derive(Clone, PartialEq)]
pub struct ShimParameter {
    /// Name of the parameter.
    pub name: String,
    /// A brief description of the parameter.
    pub description: Option<String>,
    /// Location of the parameter (query, path, header, cookie, querystring).
    pub parameter_in: String,
    /// Legacy Swagger 2.0 primitive type (e.g. string, integer).
    pub schema_type: Option<String>,
    /// Legacy Swagger 2.0 format modifier (e.g. int64, date-time).
    pub format: Option<String>,
    /// Legacy Swagger 2.0 array item schema.
    pub items: Option<Box<ShimParameterItems>>,
    /// Whether the parameter is required.
    pub required: bool,
    /// Whether the parameter is deprecated.
    pub deprecated: bool,
    /// Schema definition (Schema Object or boolean).
    pub schema: Option<SchemaOrBool>,
    /// Content map (OAS 3.x complex parameter serialization).
    /// Mutually exclusive with `schema`.
    pub content: Option<BTreeMap<String, Content>>,
    /// Serialization style (OAS 3.x).
    pub style: Option<String>,
    /// Explode modifier (OAS 3.x).
    pub explode: Option<bool>,
    /// Allow reserved characters (OAS 3.x).
    pub allow_reserved: Option<bool>,
    /// Allow empty values for query parameters (deprecated).
    pub allow_empty_value: Option<bool>,
    /// Collection format (OAS 2.0 compatibility).
    pub collection_format: Option<String>,
    /// Single example value for the parameter.
    pub example: Option<JsonValue>,
    /// Multiple example values for the parameter.
    pub examples: Option<BTreeMap<String, JsonValue>>,
    /// Raw JSON parameter object for accessing unmodeled OAS 3.2 fields.
    pub raw: JsonValue,
}

struct ResolvedParameter {
    param: ShimParameter,
    components: Option<ShimComponents>,
    base_uri: Option<Url>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Default)]
struct ShimParameterData {
    /// Name of the parameter.
    pub name: String,
    /// A brief description of the parameter.
    pub description: Option<String>,
    /// Location of the parameter (query, path, header, cookie, querystring).
    #[serde(rename = "in")]
    pub parameter_in: String,
    /// Legacy Swagger 2.0 primitive type (e.g. string, integer).
    #[serde(rename = "type")]
    pub schema_type: Option<String>,
    /// Legacy Swagger 2.0 format modifier (e.g. int64, date-time).
    pub format: Option<String>,
    /// Legacy Swagger 2.0 array item schema.
    pub items: Option<Box<ShimParameterItems>>,
    /// Whether the parameter is required.
    #[serde(default)]
    pub required: bool,
    /// Whether the parameter is deprecated.
    #[serde(default)]
    pub deprecated: bool,
    /// Schema definition (Schema Object or boolean).
    pub schema: Option<SchemaOrBool>,
    /// Content map (OAS 3.x complex parameter serialization).
    /// Mutually exclusive with `schema`.
    pub content: Option<BTreeMap<String, Content>>,
    /// Serialization style (OAS 3.x).
    pub style: Option<String>,
    /// Explode modifier (OAS 3.x).
    pub explode: Option<bool>,
    /// Allow reserved characters (OAS 3.x).
    #[serde(rename = "allowReserved", default)]
    pub allow_reserved: Option<bool>,
    /// Allow empty values for query parameters (deprecated).
    #[serde(rename = "allowEmptyValue", default)]
    pub allow_empty_value: Option<bool>,
    /// Collection format (OAS 2.0 compatibility).
    #[serde(rename = "collectionFormat")]
    pub collection_format: Option<String>,
    /// Single example value for the parameter.
    #[serde(default)]
    pub example: Option<JsonValue>,
    /// Multiple example values for the parameter.
    #[serde(default)]
    pub examples: Option<BTreeMap<String, JsonValue>>,
}

impl Default for ShimParameter {
    fn default() -> Self {
        let data = ShimParameterData::default();
        Self {
            name: data.name,
            description: data.description,
            parameter_in: data.parameter_in,
            schema_type: data.schema_type,
            format: data.format,
            items: data.items,
            required: data.required,
            deprecated: data.deprecated,
            schema: data.schema,
            content: data.content,
            style: data.style,
            explode: data.explode,
            allow_reserved: data.allow_reserved,
            allow_empty_value: data.allow_empty_value,
            collection_format: data.collection_format,
            example: data.example,
            examples: data.examples,
            raw: JsonValue::Null,
        }
    }
}

impl<'de> Deserialize<'de> for ShimParameter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = JsonValue::deserialize(deserializer)?;
        let data = serde_json::from_value::<ShimParameterData>(raw.clone())
            .map_err(|e| DeError::custom(format!("Failed to parse Parameter object: {}", e)))?;
        Ok(Self {
            name: data.name,
            description: data.description,
            parameter_in: data.parameter_in,
            schema_type: data.schema_type,
            format: data.format,
            items: data.items,
            required: data.required,
            deprecated: data.deprecated,
            schema: data.schema,
            content: data.content,
            style: data.style,
            explode: data.explode,
            allow_reserved: data.allow_reserved,
            allow_empty_value: data.allow_empty_value,
            collection_format: data.collection_format,
            example: data.example,
            examples: data.examples,
            raw,
        })
    }
}

impl Serialize for ShimParameter {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let data = ShimParameterData {
            name: self.name.clone(),
            description: self.description.clone(),
            parameter_in: self.parameter_in.clone(),
            schema_type: self.schema_type.clone(),
            format: self.format.clone(),
            items: self.items.clone(),
            required: self.required,
            deprecated: self.deprecated,
            schema: self.schema.clone(),
            content: self.content.clone(),
            style: self.style.clone(),
            explode: self.explode,
            allow_reserved: self.allow_reserved,
            allow_empty_value: self.allow_empty_value,
            collection_format: self.collection_format.clone(),
            example: self.example.clone(),
            examples: self.examples.clone(),
        };
        data.serialize(serializer)
    }
}

/// Legacy Swagger 2.0 array item schema for parameters.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct ShimParameterItems {
    /// Item type (string, integer, etc.).
    #[serde(rename = "type")]
    pub schema_type: Option<String>,
    /// Item format (int64, date-time, etc.).
    pub format: Option<String>,
    /// Nested array item schema for multi-dimensional arrays.
    pub items: Option<Box<ShimParameterItems>>,
}

// Manual Debug implementation to avoid strict type bounds on fields derived by macros sometimes.
impl fmt::Debug for ShimParameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShimParameter")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("parameter_in", &self.parameter_in)
            .field("schema_type", &self.schema_type)
            .field("format", &self.format)
            .field("required", &self.required)
            .field("deprecated", &self.deprecated)
            .field("style", &self.style)
            .field("explode", &self.explode)
            .field("allow_reserved", &self.allow_reserved)
            .field("allow_empty_value", &self.allow_empty_value)
            .field("schema", &self.schema.as_ref().map(|_| "Some(Schema)"))
            .field("content", &self.content.as_ref().map(|_| "Some(Content)"))
            .finish()
    }
}

/// Resolves a list of OpenAPI parameters into internal `RouteParam` structs.
///
/// `is_oas3` toggles strict OpenAPI 3.x requirements for `schema`/`content`.
/// Swagger 2.0 legacy parameters use `type`/`format`/`items` when present.
pub fn resolve_parameters(
    params: &[RefOr<ShimParameter>],
    components: Option<&ShimComponents>,
    is_oas3: bool,
) -> AppResult<Vec<RouteParam>> {
    resolve_parameters_with_registry(params, components, is_oas3, None, None)
}

/// Resolves parameters with optional external reference resolution.
pub fn resolve_parameters_with_registry(
    params: &[RefOr<ShimParameter>],
    components: Option<&ShimComponents>,
    is_oas3: bool,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<Vec<RouteParam>> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for param_or_ref in params {
        let mut override_components = None;
        let mut override_base = None;
        let (param_opt, ref_description) = match param_or_ref {
            RefOr::T(param) => (Some(param.clone()), None),
            RefOr::Ref(r) => {
                let resolved = resolve_parameter_ref(r, components, registry, base_uri);
                if let Some(resolved) = resolved {
                    override_components = resolved.components;
                    override_base = resolved.base_uri;
                    (
                        Some(resolved.param),
                        (!r.description.is_empty()).then(|| r.description.clone()),
                    )
                } else {
                    (
                        None,
                        (!r.description.is_empty()).then(|| r.description.clone()),
                    )
                }
            }
        };

        if let Some(mut param) = param_opt {
            if let Some(desc) = ref_description {
                param.description = Some(desc);
            }
            if should_ignore_header_param(&param) {
                continue;
            }
            let location_key = param.parameter_in.to_ascii_lowercase();
            let name_key = if location_key == "header" {
                param.name.to_ascii_lowercase()
            } else {
                param.name.clone()
            };
            let key = (name_key, location_key);
            if !seen.insert(key.clone()) {
                return Err(AppError::General(format!(
                    "Duplicate parameter '{}' in location '{}'",
                    param.name, param.parameter_in
                )));
            }
            let components_ctx = override_components.as_ref().or(components);
            let base_ctx = override_base.as_ref().or(base_uri);
            let route_param = process_parameter_with_registry(
                &param,
                components_ctx,
                is_oas3,
                registry,
                base_ctx,
            )?;
            result.push(route_param);
        }
    }
    Ok(result)
}

/// Determines whether a parameter should be ignored because it targets a reserved HTTP header.
fn should_ignore_header_param(param: &ShimParameter) -> bool {
    if !param.parameter_in.eq_ignore_ascii_case("header") {
        return false;
    }

    matches!(
        param.name.to_ascii_lowercase().as_str(),
        "accept" | "content-type" | "authorization"
    )
}

/// Helper to resolve a `Ref` to its target Parameter definition.
fn resolve_parameter_ref(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<ResolvedParameter> {
    if let Some((comps, self_uri)) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))
    {
        if let Some(ref_name) = extract_component_name(&r.ref_location, self_uri, "parameters") {
            if let Some(param_json) = comps.extra.get("parameters").and_then(|p| p.get(&ref_name)) {
                if let Ok(param) = serde_json::from_value::<ShimParameter>(param_json.clone()) {
                    return Some(ResolvedParameter {
                        param,
                        components: None,
                        base_uri: None,
                    });
                }
            }
        }
    }

    if let Some(registry) = registry {
        if let Some((raw, comps_override, base_override)) =
            registry.resolve_component_ref_with_components(&r.ref_location, base_uri, "parameters")
        {
            if let Ok(param) = serde_json::from_value::<ShimParameter>(raw) {
                return Some(ResolvedParameter {
                    param,
                    components: comps_override,
                    base_uri: base_override,
                });
            }
        }
    }

    None
}

/// Helper to convert a resolved ShimParameter to our internal RouteParam.
/// Implements default resolution logic for style/explode based on the OAS 3.2.0 spec.
fn process_parameter(
    param: &ShimParameter,
    components: Option<&ShimComponents>,
    is_oas3: bool,
) -> AppResult<RouteParam> {
    process_parameter_with_registry(param, components, is_oas3, None, None)
}

fn filter_extensions(raw: &JsonValue) -> BTreeMap<String, JsonValue> {
    raw.as_object()
        .map(|obj| {
            obj.iter()
                .filter(|(key, _)| key.starts_with("x-"))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn process_parameter_with_registry(
    param: &ShimParameter,
    components: Option<&ShimComponents>,
    is_oas3: bool,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<RouteParam> {
    let name = param.name.clone();
    if is_oas3 && param.example.is_some() && param.examples.is_some() {
        return Err(AppError::General(format!(
            "Parameter '{}' must not define both 'example' and 'examples'",
            name
        )));
    }
    if is_oas3 {
        if let Some(examples) = &param.examples {
            for (example_name, example_val) in examples {
                validate_example_object_value(
                    example_val,
                    &format!("parameter '{}'.examples.{}", name, example_name),
                )?;
            }
        }
    }
    if param.schema.is_some() && param.content.is_some() {
        return Err(AppError::General(format!(
            "Parameter '{}' cannot specify both 'schema' and 'content'",
            name
        )));
    }
    if param.content.is_some()
        && (param.style.is_some()
            || param.explode.is_some()
            || param.allow_reserved.is_some()
            || param.collection_format.is_some())
    {
        return Err(AppError::General(format!(
            "Parameter '{}' uses 'content' and must not define style/explode/allowReserved/collectionFormat", 
            name
        )));
    }
    let source = match param.parameter_in.as_str() {
        "path" => ParamSource::Path,
        "query" => ParamSource::Query,
        // OAS 3.2.0 Support: entire query string
        "querystring" => ParamSource::QueryString,
        "header" => ParamSource::Header,
        "cookie" => ParamSource::Cookie,
        _ => ParamSource::Query,
    };

    let allow_empty_value = param.allow_empty_value.unwrap_or(false);
    if allow_empty_value && source != ParamSource::Query {
        return Err(AppError::General(format!(
            "Parameter '{}' uses allowEmptyValue but is not in 'query'",
            name
        )));
    }

    if source == ParamSource::Path && !param.required {
        return Err(AppError::General(format!(
            "Path parameter '{}' must set required: true",
            name
        )));
    }

    let required = if source == ParamSource::Path {
        true
    } else {
        param.required
    };

    if source == ParamSource::QueryString {
        if param.schema.is_some() {
            return Err(AppError::General(format!(
                "Querystring parameter '{}' must use 'content' instead of 'schema'",
                name
            )));
        }
        let content_len = param.content.as_ref().map(|m| m.len()).unwrap_or(0);
        if content_len != 1 {
            return Err(AppError::General(format!(
                "Querystring parameter '{}' must define exactly one media type in 'content'",
                name
            )));
        }
    } else if let Some(content) = &param.content {
        if content.len() != 1 {
            return Err(AppError::General(format!(
                "Parameter '{}' must define exactly one media type in 'content'",
                name
            )));
        }
    }

    // Determine style based on OAS3 fields, defaulting to OAS2 logic if missing
    let style = resolve_style(
        param.style.as_deref(),
        param.collection_format.as_deref(),
        &source,
    );

    validate_style_for_location(&name, &source, &style)?;
    if is_oas3 {
        let kind = infer_param_value_kind(param);
        validate_style_for_type(&name, &style, kind)?;
    }

    // Determine explode. Defaults depend on style.
    let explode = resolve_explode(param.explode, &style);

    // Resolve type from `schema` OR `content` OR legacy Swagger 2.0 fields.
    let raw_schema_json = param.raw.get("schema");
    let (ty, content_media_type) = if let Some(schema_ref) = &param.schema {
        if schema_ref.is_false() {
            return Err(AppError::General(format!(
                "Parameter '{}' schema is 'false' and cannot be satisfied",
                name
            )));
        }
        let ty = match schema_ref {
            SchemaOrBool::Schema(schema) => {
                map_schema_to_rust_type_with_raw(schema, required, raw_schema_json)?
            }
            SchemaOrBool::Bool(true) => "String".to_string(),
            SchemaOrBool::Bool(false) => unreachable!("handled above"),
        };
        (ty, None)
    } else if let Some(content) = &param.content {
        // OAS 3.x: "The map MUST only contain one entry."
        if let Some((media_type, media_obj)) = content.iter().next() {
            let raw_media = raw_media_for_type(
                param.raw.get("content").and_then(|v| v.as_object()),
                media_type,
                components,
                registry,
                base_uri,
            );
            let raw_schema_json = raw_media.as_ref().and_then(|m| m.get("schema"));
            let raw_schema = raw_media.as_ref().and_then(extract_media_schema);
            let item_schema = raw_media.as_ref().and_then(extract_item_schema);
            let ty = if let Some(s) = &media_obj.schema {
                map_schema_to_rust_type_with_raw(s, required, raw_schema_json)?
            } else if let Some(schema_ref) = raw_schema.as_ref() {
                map_schema_to_rust_type_with_raw(schema_ref, required, raw_schema_json)?
            } else if let Some(item_schema) = item_schema {
                let inner = map_schema_to_rust_type(&RefOr::T(item_schema), true)?;
                if is_sequential_media_type(media_type) {
                    format!("Vec<{}>", inner)
                } else {
                    inner
                }
            } else {
                "serde_json::Value".to_string()
            };
            (ty, Some(ContentMediaType::from_media_type(media_type)))
        } else {
            return Err(AppError::General(format!(
                "Parameter '{}' has empty content map",
                name
            )));
        }
    } else if !is_oas3 {
        if let Some(legacy_ty) = map_legacy_parameter_type(param, required) {
            (legacy_ty, None)
        } else {
            ("String".to_string(), None)
        }
    } else {
        return Err(AppError::General(format!(
            "Parameter '{}' must define either 'schema' or 'content'",
            name
        )));
    };

    let example = extract_param_example(param, components, registry, base_uri);

    let raw_schema = if param.schema.is_some() {
        param.raw.get("schema").cloned()
    } else if param.content.is_some() {
        raw_media_for_first_entry(
            param.raw.get("content"),
            param.content.as_ref(),
            components,
            registry,
            base_uri,
        )
        .and_then(|media| media.get("schema").cloned())
    } else {
        None
    };

    Ok(RouteParam {
        name,
        description: param.description.clone(),
        source,
        ty,
        content_media_type,
        style,
        explode,
        allow_reserved: param.allow_reserved.unwrap_or(false),
        deprecated: param.deprecated,
        allow_empty_value,
        example,
        raw_schema,
        extensions: filter_extensions(&param.raw),
    })
}

/// Extracts an example value for a parameter, including nested content examples.
fn extract_param_example(
    param: &ShimParameter,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<ExampleValue> {
    if let Some(example) = &param.example {
        return Some(ExampleValue::data(example.clone()));
    }

    if let Some(examples) = param.examples.as_ref() {
        for value in examples.values() {
            if let Some(extracted) = extract_example_value_with_ref_overrides(value, components) {
                return Some(extracted);
            }
        }
    }

    if let Some(schema_ref) = &param.schema {
        if let Some(schema) = schema_ref.as_schema() {
            if let Some(schema_example) = extract_schema_example(schema, components) {
                return Some(ExampleValue::data(schema_example));
            }
        }
    }

    extract_content_example(
        param.content.as_ref(),
        param.raw.get("content"),
        components,
        registry,
        base_uri,
    )
}

/// Extracts an example from a content map (media type object), if present.
fn extract_content_example(
    content: Option<&BTreeMap<String, Content>>,
    raw_content: Option<&JsonValue>,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<ExampleValue> {
    if let Some(raw_media) =
        raw_media_for_first_entry(raw_content, content, components, registry, base_uri)
    {
        if let Some(example) = extract_raw_media_example(&raw_media, components) {
            return Some(example);
        }
    }

    let content = content?;
    let (_, media) = content.iter().next()?;

    if let Some(example) = &media.example {
        return Some(ExampleValue::data(example.clone()));
    }

    for example_ref in media.examples.values() {
        if let Some(val) = extract_content_example_ref_or(example_ref, components) {
            return Some(val);
        }
    }

    None
}

fn raw_media_for_first_entry(
    raw_content: Option<&JsonValue>,
    content: Option<&BTreeMap<String, Content>>,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<JsonValue> {
    let raw_map = raw_content.and_then(|v| v.as_object())?;
    let key = content
        .and_then(|map| map.keys().next().cloned())
        .or_else(|| raw_map.keys().next().cloned())?;
    raw_media_for_type(Some(raw_map), &key, components, registry, base_uri)
}

fn extract_raw_media_example(
    raw_media: &JsonValue,
    components: Option<&ShimComponents>,
) -> Option<ExampleValue> {
    if let Some(example) = raw_media.get("example") {
        return Some(ExampleValue::data(example.clone()));
    }
    if let Some(examples) = raw_media.get("examples").and_then(|v| v.as_object()) {
        for value in examples.values() {
            if let Some(extracted) = extract_example_value_with_ref_overrides(value, components) {
                return Some(extracted);
            }
        }
    }
    None
}

/// Extracts a concrete example value from a media-type Example ref or inline object.
fn extract_content_example_ref_or(
    example_ref: &RefOr<Example>,
    components: Option<&ShimComponents>,
) -> Option<ExampleValue> {
    match example_ref {
        RefOr::T(example) => {
            let summary = (!example.summary.is_empty()).then(|| example.summary.clone());
            let description =
                (!example.description.is_empty()).then(|| example.description.clone());
            example
                .value
                .clone()
                .map(|val| ExampleValue::data_with_meta(val, summary.clone(), description.clone()))
                .or_else(|| {
                    (!example.external_value.is_empty()).then(|| {
                        ExampleValue::external_with_meta(
                            json!(example.external_value.clone()),
                            summary.clone(),
                            description.clone(),
                        )
                    })
                })
        }
        RefOr::Ref(r) => {
            let summary = (!r.summary.is_empty()).then(|| r.summary.clone());
            let description = (!r.description.is_empty()).then(|| r.description.clone());
            resolve_example_ref(
                &r.ref_location,
                components,
                &mut std::collections::HashSet::new(),
            )
            .map(|example| example.with_overrides(summary, description))
        }
    }
}

fn extract_example_value_with_ref_overrides(
    value: &JsonValue,
    components: Option<&ShimComponents>,
) -> Option<ExampleValue> {
    if let Some(obj) = value.as_object() {
        let summary = obj
            .get("summary")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let description = obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if let Some(ref_str) = obj.get("$ref").and_then(|v| v.as_str()) {
            let mut visiting = std::collections::HashSet::new();
            return resolve_example_ref(ref_str, components, &mut visiting)
                .map(|example| example.with_overrides(summary, description));
        }
    }

    extract_example_value(value, components, &mut std::collections::HashSet::new())
}

fn extract_example_value(
    value: &JsonValue,
    components: Option<&ShimComponents>,
    visiting: &mut std::collections::HashSet<String>,
) -> Option<ExampleValue> {
    if let Some(obj) = value.as_object() {
        let (summary, description) = example_meta_from_obj(obj);
        if let Some(ref_str) = obj.get("$ref").and_then(|v| v.as_str()) {
            return resolve_example_ref(ref_str, components, visiting);
        }
        if let Some(val) = obj.get("dataValue") {
            return Some(ExampleValue::data_with_meta(
                val.clone(),
                summary,
                description,
            ));
        }
        if let Some(val) = obj.get("serializedValue") {
            return Some(ExampleValue::serialized_with_meta(
                val.clone(),
                summary,
                description,
            ));
        }
        if let Some(val) = obj.get("externalValue") {
            return Some(ExampleValue::external_with_meta(
                val.clone(),
                summary,
                description,
            ));
        }
        if let Some(val) = obj.get("value") {
            return Some(ExampleValue::data_with_meta(
                val.clone(),
                summary,
                description,
            ));
        }
        return None;
    }

    if !value.is_null() {
        return Some(ExampleValue::data(value.clone()));
    }

    None
}

fn example_meta_from_obj(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> (Option<String>, Option<String>) {
    let summary = obj
        .get("summary")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    (summary, description)
}

fn extract_schema_example(
    schema_ref: &RefOr<Schema>,
    components: Option<&ShimComponents>,
) -> Option<JsonValue> {
    match schema_ref {
        RefOr::T(schema) => extract_example_from_schema(schema),
        RefOr::Ref(r) => resolve_schema_example_ref(&r.ref_location, components),
    }
}

fn extract_example_from_schema(schema: &Schema) -> Option<JsonValue> {
    match schema {
        Schema::Object(obj) => obj
            .example
            .clone()
            .or_else(|| obj.examples.first().cloned()),
        Schema::Array(arr) => arr
            .example
            .clone()
            .or_else(|| arr.examples.first().cloned()),
        Schema::OneOf(one_of) => one_of
            .example
            .clone()
            .or_else(|| one_of.examples.first().cloned()),
        Schema::AnyOf(any_of) => any_of
            .example
            .clone()
            .or_else(|| any_of.examples.first().cloned()),
        Schema::AllOf(all_of) => all_of
            .example
            .clone()
            .or_else(|| all_of.examples.first().cloned()),
        _ => None,
    }
}

fn resolve_schema_example_ref(
    ref_str: &str,
    components: Option<&ShimComponents>,
) -> Option<JsonValue> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let name = extract_component_name(ref_str, self_uri, "schemas")?;
    let schema_json = comps.extra.get("schemas").and_then(|s| s.get(&name))?;
    extract_schema_example_from_value(schema_json)
}

fn extract_schema_example_from_value(value: &JsonValue) -> Option<JsonValue> {
    let obj = value.as_object()?;
    if let Some(example) = obj.get("example") {
        return Some(example.clone());
    }
    obj.get("examples")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first().cloned())
}

fn raw_media_for_type(
    content: Option<&serde_json::Map<String, JsonValue>>,
    media_type: &str,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<JsonValue> {
    let map = content?;
    let media = map.get(media_type)?;
    resolve_media_type_ref(media, components, registry, base_uri, &mut HashSet::new()).or_else(
        || {
            if media.as_object().is_some() {
                Some(media.clone())
            } else {
                None
            }
        },
    )
}

fn resolve_media_type_ref(
    raw_media: &JsonValue,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
    visiting: &mut HashSet<String>,
) -> Option<JsonValue> {
    let obj = raw_media.as_object()?;
    let ref_str = obj.get("$ref")?.as_str()?;
    if !visiting.insert(ref_str.to_string()) {
        return None;
    }

    if let Some((comps, self_uri)) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))
    {
        if let Some(name) = extract_component_name(ref_str, self_uri, "mediaTypes") {
            if let Some(media_types) = comps.extra.get("mediaTypes").and_then(|v| v.as_object()) {
                if let Some(resolved) = media_types.get(&name) {
                    let value =
                        resolve_media_type_ref(resolved, components, registry, base_uri, visiting)
                            .unwrap_or_else(|| resolved.clone());
                    visiting.remove(ref_str);
                    return Some(value);
                }
            }
        }
    }

    if let Some(registry) = registry {
        if let Some((resolved, comps_override, base_override)) =
            registry.resolve_component_ref_with_components(ref_str, base_uri, "mediaTypes")
        {
            let next_components = comps_override.as_ref().or(components);
            let next_base = base_override.as_ref().or(base_uri);
            let value = resolve_media_type_ref(
                &resolved,
                next_components,
                Some(registry),
                next_base,
                visiting,
            )
            .unwrap_or_else(|| resolved.clone());
            visiting.remove(ref_str);
            return Some(value);
        }
    }

    visiting.remove(ref_str);
    None
}

fn extract_item_schema(raw_media: &JsonValue) -> Option<Schema> {
    let item_schema = raw_media.get("itemSchema")?;
    serde_json::from_value::<Schema>(item_schema.clone()).ok()
}

fn extract_media_schema(raw_media: &JsonValue) -> Option<RefOr<Schema>> {
    let schema_val = raw_media.get("schema")?;
    serde_json::from_value::<RefOr<Schema>>(schema_val.clone()).ok()
}

fn normalize_media_type(media_type: &str) -> String {
    media_type
        .split(';')
        .next()
        .unwrap_or(media_type)
        .trim()
        .to_ascii_lowercase()
}

fn is_sequential_media_type(media_type: &str) -> bool {
    let normalized = normalize_media_type(media_type);
    matches!(
        normalized.as_str(),
        "application/jsonl"
            | "application/x-ndjson"
            | "application/json-seq"
            | "application/geo+json-seq"
            | "text/event-stream"
            | "multipart/mixed"
            | "multipart/byteranges"
    ) || normalized.ends_with("+jsonl")
        || normalized.ends_with("+ndjson")
        || normalized.ends_with("+json-seq")
}

fn resolve_example_ref(
    ref_str: &str,
    components: Option<&ShimComponents>,
    visiting: &mut std::collections::HashSet<String>,
) -> Option<ExampleValue> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let name = extract_component_name(ref_str, self_uri, "examples")?;
    if !visiting.insert(name.clone()) {
        return None;
    }
    let example_json = comps.extra.get("examples").and_then(|e| e.get(&name))?;
    let resolved = extract_example_value(example_json, components, visiting);
    visiting.remove(&name);
    resolved
}

/// Resolves the parameter style.
///
/// Priorities:
/// 1. Explicit `style` (OAS 3).
/// 2. Mapped `collectionFormat` (OAS 2).
/// 3. Default based on `in` location (OAS 3 spec).
fn resolve_style(
    style_str: Option<&str>,
    collection_format: Option<&str>,
    source: &ParamSource,
) -> Option<ParamStyle> {
    if let Some(s) = style_str {
        return match s {
            "matrix" => Some(ParamStyle::Matrix),
            "label" => Some(ParamStyle::Label),
            "form" => Some(ParamStyle::Form),
            "cookie" => Some(ParamStyle::Cookie),
            "simple" => Some(ParamStyle::Simple),
            "spaceDelimited" => Some(ParamStyle::SpaceDelimited),
            "pipeDelimited" => Some(ParamStyle::PipeDelimited),
            "deepObject" => Some(ParamStyle::DeepObject),
            _ => None,
        };
    }

    if let Some(cf) = collection_format {
        // Map OAS 2.0 collectionFormat to style
        return match cf {
            "csv" => match source {
                ParamSource::Query | ParamSource::Cookie | ParamSource::QueryString => {
                    Some(ParamStyle::Form)
                }
                ParamSource::Path | ParamSource::Header => Some(ParamStyle::Simple),
            },
            "ssv" => Some(ParamStyle::SpaceDelimited),
            "tsv" => Some(ParamStyle::SpaceDelimited), // Approximate mapping
            "pipes" => Some(ParamStyle::PipeDelimited),
            "multi" => Some(ParamStyle::Form), // 'multi' implies form with explode=true
            _ => None,
        };
    }

    // OAS 3.2.0 Defaults
    match source {
        // `query` and `cookie` (and implicit `querystring`) default to Form
        ParamSource::Query | ParamSource::QueryString => Some(ParamStyle::Form),
        ParamSource::Path => Some(ParamStyle::Simple),
        ParamSource::Header => Some(ParamStyle::Simple),
        ParamSource::Cookie => Some(ParamStyle::Form),
    }
}

/// Resolves the explode property.
///
/// Defaults:
/// - `style: form` -> true
/// - `style: cookie` -> true (OAS 3.2 spec note: "When style is form or cookie, the default value is true")
/// - Others -> false
fn resolve_explode(explicit: Option<bool>, style: &Option<ParamStyle>) -> bool {
    if let Some(e) = explicit {
        return e;
    }

    match style {
        Some(ParamStyle::Form) => true,
        Some(ParamStyle::Cookie) => true,
        _ => false,
    }
}

fn map_legacy_parameter_type(param: &ShimParameter, required: bool) -> Option<String> {
    let schema_type = param.schema_type.as_deref()?;
    let ty = map_legacy_schema_type(schema_type, param.format.as_deref(), param.items.as_deref());

    if required {
        Some(ty)
    } else {
        Some(format!("Option<{}>", ty))
    }
}

fn map_legacy_schema_type(
    schema_type: &str,
    format: Option<&str>,
    items: Option<&ShimParameterItems>,
) -> String {
    match schema_type {
        "integer" => match format {
            Some("int64") => "i64".to_string(),
            _ => "i32".to_string(),
        },
        "number" => match format {
            Some("float") => "f32".to_string(),
            Some("double") => "f64".to_string(),
            _ => "f64".to_string(),
        },
        "boolean" => "bool".to_string(),
        "string" => match format {
            Some("uuid") => "Uuid".to_string(),
            Some("date-time") => "DateTime".to_string(),
            Some("date") => "NaiveDate".to_string(),
            Some("password") => "Secret<String>".to_string(),
            Some("byte") | Some("binary") => "Vec<u8>".to_string(),
            _ => "String".to_string(),
        },
        "array" => {
            let inner = if let Some(item) = items {
                if let Some(item_type) = item.schema_type.as_deref() {
                    map_legacy_schema_type(item_type, item.format.as_deref(), item.items.as_deref())
                } else {
                    "serde_json::Value".to_string()
                }
            } else {
                "serde_json::Value".to_string()
            };
            format!("Vec<{}>", inner)
        }
        "object" => "serde_json::Value".to_string(),
        "file" => "Vec<u8>".to_string(),
        _ => "serde_json::Value".to_string(),
    }
}

fn validate_style_for_location(
    name: &str,
    source: &ParamSource,
    style: &Option<ParamStyle>,
) -> AppResult<()> {
    let Some(style) = style else {
        return Ok(());
    };

    let is_allowed = match source {
        ParamSource::Path => matches!(
            style,
            ParamStyle::Matrix | ParamStyle::Label | ParamStyle::Simple
        ),
        ParamSource::Query | ParamSource::QueryString => matches!(
            style,
            ParamStyle::Form
                | ParamStyle::SpaceDelimited
                | ParamStyle::PipeDelimited
                | ParamStyle::DeepObject
        ),
        ParamSource::Header => matches!(style, ParamStyle::Simple),
        ParamSource::Cookie => matches!(style, ParamStyle::Form | ParamStyle::Cookie),
    };

    if !is_allowed {
        return Err(AppError::General(format!(
            "Parameter '{}' uses style '{}' which is not allowed for {:?}. Allowed styles: {}",
            name,
            style_name(style),
            source,
            allowed_style_names(source).join(", ")
        )));
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParamValueKind {
    Primitive,
    Array,
    Object,
    Unknown,
}

fn infer_param_value_kind(param: &ShimParameter) -> ParamValueKind {
    if let Some(schema_ref) = &param.schema {
        return match schema_ref {
            SchemaOrBool::Schema(schema) => infer_param_kind_from_schema(schema),
            SchemaOrBool::Bool(_) => ParamValueKind::Unknown,
        };
    }

    if let Some(content) = &param.content {
        if let Some((_, media)) = content.iter().next() {
            if let Some(schema_ref) = media.schema.as_ref() {
                return infer_param_kind_from_schema(schema_ref);
            }
        }
    }

    if let Some(schema_type) = &param.schema_type {
        return infer_param_kind_from_legacy_type(schema_type);
    }

    if param.items.is_some() {
        return ParamValueKind::Array;
    }

    ParamValueKind::Unknown
}

fn infer_param_kind_from_schema(schema: &RefOr<Schema>) -> ParamValueKind {
    match schema {
        RefOr::Ref(_) => ParamValueKind::Unknown,
        RefOr::T(schema) => match schema {
            Schema::Array(_) => ParamValueKind::Array,
            Schema::Object(obj) => infer_param_kind_from_schema_type(&obj.schema_type),
            Schema::OneOf(_) | Schema::AnyOf(_) | Schema::AllOf(_) => ParamValueKind::Unknown,
            _ => ParamValueKind::Unknown,
        },
    }
}

fn infer_param_kind_from_schema_type(schema_type: &SchemaType) -> ParamValueKind {
    match schema_type {
        SchemaType::Type(Type::Array) | SchemaType::Array(_) => ParamValueKind::Array,
        SchemaType::Type(Type::Object) => ParamValueKind::Object,
        SchemaType::Type(Type::String)
        | SchemaType::Type(Type::Number)
        | SchemaType::Type(Type::Integer)
        | SchemaType::Type(Type::Boolean)
        | SchemaType::Type(Type::Null) => ParamValueKind::Primitive,
        SchemaType::AnyValue => ParamValueKind::Unknown,
    }
}

fn infer_param_kind_from_legacy_type(schema_type: &str) -> ParamValueKind {
    match schema_type {
        "array" => ParamValueKind::Array,
        "object" => ParamValueKind::Object,
        "string" | "number" | "integer" | "boolean" => ParamValueKind::Primitive,
        _ => ParamValueKind::Unknown,
    }
}

fn validate_style_for_type(
    name: &str,
    style: &Option<ParamStyle>,
    kind: ParamValueKind,
) -> AppResult<()> {
    let Some(style) = style else {
        return Ok(());
    };

    if matches!(kind, ParamValueKind::Unknown) {
        return Ok(());
    }

    match style {
        ParamStyle::DeepObject => {
            if kind != ParamValueKind::Object {
                return Err(AppError::General(format!(
                    "Parameter '{}' uses style 'deepObject' but is not an object",
                    name
                )));
            }
        }
        ParamStyle::SpaceDelimited | ParamStyle::PipeDelimited => {
            if kind == ParamValueKind::Primitive {
                return Err(AppError::General(format!(
                    "Parameter '{}' uses style '{}' but is not array/object",
                    name,
                    style_name(style)
                )));
            }
        }
        _ => {}
    }

    Ok(())
}

fn style_name(style: &ParamStyle) -> &'static str {
    match style {
        ParamStyle::Matrix => "matrix",
        ParamStyle::Label => "label",
        ParamStyle::Form => "form",
        ParamStyle::Cookie => "cookie",
        ParamStyle::Simple => "simple",
        ParamStyle::SpaceDelimited => "spaceDelimited",
        ParamStyle::PipeDelimited => "pipeDelimited",
        ParamStyle::DeepObject => "deepObject",
    }
}

fn allowed_style_names(source: &ParamSource) -> &'static [&'static str] {
    match source {
        ParamSource::Path => &["matrix", "label", "simple"],
        ParamSource::Query | ParamSource::QueryString => {
            &["form", "spaceDelimited", "pipeDelimited", "deepObject"]
        }
        ParamSource::Header => &["simple"],
        ParamSource::Cookie => &["form", "cookie"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::{ExampleValue, ParamSource, ParamStyle};
    use utoipa::openapi::schema::{Schema, Type};
    use utoipa::openapi::{ContentBuilder, ObjectBuilder, Ref};

    fn string_schema() -> Option<SchemaOrBool> {
        Some(Schema::Object(ObjectBuilder::new().schema_type(Type::String).build()).into())
    }

    fn object_schema() -> Option<SchemaOrBool> {
        Some(Schema::Object(ObjectBuilder::new().schema_type(Type::Object).build()).into())
    }

    fn array_schema() -> Option<SchemaOrBool> {
        Some(Schema::Object(ObjectBuilder::new().schema_type(Type::Array).build()).into())
    }

    #[test]
    fn test_resolve_parameters_defaults_query() {
        // Case: Query param, no style/explode specified.
        // Expect: Style=Form, Explode=True.
        let param = ShimParameter {
            name: "q".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(processed.source, ParamSource::Query);
        assert_eq!(processed.style, Some(ParamStyle::Form));
        assert_eq!(processed.explode, true); // Form defaults to true
    }

    #[test]
    fn test_oas3_requires_schema_or_content() {
        let param = ShimParameter {
            name: "missing".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: None,
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        assert!(format!("{err}").contains("must define either 'schema' or 'content'"));
    }

    #[test]
    fn test_oas3_example_and_examples_conflict() {
        let mut examples = BTreeMap::new();
        examples.insert("ex".to_string(), serde_json::json!({ "value": "two" }));
        let param = ShimParameter {
            name: "q".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: Some(serde_json::json!("one")),
            examples: Some(examples),
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        assert!(format!("{err}").contains("must not define both 'example' and 'examples'"));
    }

    #[test]
    fn test_swagger2_parameter_integer_format() {
        let param = ShimParameter {
            name: "limit".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: Some("integer".to_string()),
            format: Some("int64".to_string()),
            items: None,
            required: true,
            deprecated: false,
            schema: None,
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, false).unwrap();
        assert_eq!(processed.ty, "i64");
    }

    #[test]
    fn test_swagger2_parameter_array_items() {
        let items = ShimParameterItems {
            schema_type: Some("string".to_string()),
            format: None,
            items: None,
        };

        let param = ShimParameter {
            name: "tags".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: Some("array".to_string()),
            format: None,
            items: Some(Box::new(items)),
            required: true,
            deprecated: false,
            schema: None,
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: Some("csv".to_string()),
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, false).unwrap();
        assert_eq!(processed.ty, "Vec<String>");
        assert_eq!(processed.style, Some(ParamStyle::Form));
    }

    #[test]
    fn test_resolve_parameters_complex_content() {
        // Case: Query param defined with `content` instead of `schema`
        // e.g. ?filter={"foo":"bar"} (application/json)
        let schema = ObjectBuilder::new().schema_type(Type::Object).build();
        let content_item = ContentBuilder::new()
            .schema(Some(RefOr::T(Schema::Object(schema))))
            .build();

        let mut content_map = BTreeMap::new();
        content_map.insert("application/json".into(), content_item);

        let param = ShimParameter {
            name: "filter".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: None,
            content: Some(content_map),
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        // Since schema is Object, map_schema_to_rust_type returns "serde_json::Value" unless typed struct ref
        assert_eq!(processed.ty, "serde_json::Value");
        assert_eq!(processed.source, ParamSource::Query);
        assert!(matches!(
            processed.content_media_type,
            Some(ContentMediaType::Json)
        ));
    }

    #[test]
    fn test_resolve_parameters_querystring_oas_3_2() {
        // Case: OAS 3.2 in: querystring requires content with exactly one media type.
        // Expect: Source=QueryString, Style=Form (Default)
        let schema = ObjectBuilder::new().schema_type(Type::Object).build();
        let content_item = ContentBuilder::new()
            .schema(Some(RefOr::T(Schema::Object(schema))))
            .build();

        let mut content_map = BTreeMap::new();
        content_map.insert("application/x-www-form-urlencoded".into(), content_item);

        let param = ShimParameter {
            name: "filter".to_string(),
            description: None,
            parameter_in: "querystring".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: None,
            content: Some(content_map),
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(processed.source, ParamSource::QueryString);
        assert_eq!(processed.style, Some(ParamStyle::Form));
        assert!(matches!(
            processed.content_media_type,
            Some(ContentMediaType::FormUrlEncoded)
        ));
    }

    #[test]
    fn test_resolve_parameters_content_example() {
        let schema = ObjectBuilder::new().schema_type(Type::Object).build();
        let content_item = ContentBuilder::new()
            .schema(Some(RefOr::T(Schema::Object(schema))))
            .example(Some(serde_json::json!({"filter": "active"})))
            .build();

        let mut content_map = BTreeMap::new();
        content_map.insert("application/json".into(), content_item);

        let param = ShimParameter {
            name: "filter".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: None,
            content: Some(content_map),
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(
            processed.example,
            Some(ExampleValue::data(serde_json::json!({"filter": "active"})))
        );
    }

    #[test]
    fn test_resolve_parameters_example_metadata() {
        let mut examples = BTreeMap::new();
        examples.insert(
            "meta".to_string(),
            serde_json::json!({
                "summary": "Short summary",
                "description": "Longer description",
                "dataValue": "active"
            }),
        );

        let param = ShimParameter {
            name: "status".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: Some(examples),
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        let example = processed.example.expect("example missing");
        assert_eq!(example.summary.as_deref(), Some("Short summary"));
        assert_eq!(example.description.as_deref(), Some("Longer description"));
        assert_eq!(example.value, serde_json::json!("active"));
    }

    #[test]
    fn test_resolve_parameters_content_schema_mapping() {
        let raw = serde_json::json!({
            "name": "filter",
            "in": "query",
            "content": {
                "application/json": {
                    "schema": {
                        "type": "string",
                        "contentMediaType": "application/json",
                        "contentSchema": {
                            "type": "integer",
                            "format": "int32"
                        }
                    }
                }
            }
        });
        let param: ShimParameter = serde_json::from_value(raw).unwrap();
        let params = vec![RefOr::T(param)];

        let resolved = resolve_parameters(&params, None, true).unwrap();
        assert_eq!(resolved[0].ty, "Option<i32>");
    }

    #[test]
    fn test_resolve_parameters_content_example_ref() {
        let schema = ObjectBuilder::new().schema_type(Type::Object).build();
        let content_item = ContentBuilder::new()
            .schema(Some(RefOr::T(Schema::Object(schema))))
            .examples_from_iter([(
                "example",
                RefOr::Ref(Ref::new("#/components/examples/FilterExample")),
            )])
            .build();

        let mut content_map = BTreeMap::new();
        content_map.insert("application/json".into(), content_item);

        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "examples".to_string(),
            serde_json::json!({
                "FilterExample": {
                    "value": {"status": "open"}
                }
            }),
        );

        let param = ShimParameter {
            name: "filter".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: None,
            content: Some(content_map),
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, Some(&components), true).unwrap();
        assert_eq!(
            processed.example,
            Some(ExampleValue::data(serde_json::json!({"status": "open"})))
        );
    }

    #[test]
    fn test_resolve_parameters_defaults_path() {
        // Case: Path param.
        // Expect: Style=Simple, Explode=False.
        let param = ShimParameter {
            name: "id".to_string(),
            description: None,
            parameter_in: "path".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(processed.source, ParamSource::Path);
        assert_eq!(processed.style, Some(ParamStyle::Simple));
        assert_eq!(processed.explode, false); // Simple defaults to false
    }

    #[test]
    fn test_cookie_style_explicit() {
        let param = ShimParameter {
            name: "session".to_string(),
            description: None,
            parameter_in: "cookie".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: Some("cookie".to_string()),
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(processed.source, ParamSource::Cookie);
        assert_eq!(processed.style, Some(ParamStyle::Cookie));
        assert_eq!(processed.explode, true);
    }

    #[test]
    fn test_cookie_style_rejects_invalid() {
        let param = ShimParameter {
            name: "session".to_string(),
            description: None,
            parameter_in: "cookie".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: Some("simple".to_string()),
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("uses style 'simple' which is not allowed"));
    }

    #[test]
    fn test_header_style_rejects_non_simple() {
        let param = ShimParameter {
            name: "X-Thing".to_string(),
            description: None,
            parameter_in: "header".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: Some("form".to_string()),
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("uses style 'form' which is not allowed"));
    }

    #[test]
    fn test_cookie_style_rejects_non_cookie_location() {
        let param = ShimParameter {
            name: "session".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: Some("cookie".to_string()),
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("uses style 'cookie' which is not allowed"));
    }

    #[test]
    fn test_path_style_rejects_form() {
        let param = ShimParameter {
            name: "id".to_string(),
            description: None,
            parameter_in: "path".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: Some("form".to_string()),
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("uses style 'form' which is not allowed"));
    }

    #[test]
    fn test_query_style_rejects_simple() {
        let param = ShimParameter {
            name: "filter".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: Some("simple".to_string()),
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("uses style 'simple' which is not allowed"));
    }

    #[test]
    fn test_query_style_deep_object_allowed() {
        let param = ShimParameter {
            name: "filter".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: object_schema(),
            content: None,
            style: Some("deepObject".to_string()),
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(processed.style, Some(ParamStyle::DeepObject));
    }

    #[test]
    fn test_query_style_deep_object_rejects_primitive() {
        let param = ShimParameter {
            name: "filter".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: Some("deepObject".to_string()),
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        assert!(format!("{err}").contains("deepObject"));
    }

    #[test]
    fn test_query_style_space_delimited_rejects_primitive() {
        let param = ShimParameter {
            name: "tags".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: Some("spaceDelimited".to_string()),
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        assert!(format!("{err}").contains("spaceDelimited"));
    }

    #[test]
    fn test_query_style_pipe_delimited_allows_array() {
        let param = ShimParameter {
            name: "tags".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: array_schema(),
            content: None,
            style: Some("pipeDelimited".to_string()),
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(processed.style, Some(ParamStyle::PipeDelimited));
    }

    #[test]
    fn test_path_parameter_requires_required_true() {
        let schema = ObjectBuilder::new().schema_type(Type::String).build();
        let param = ShimParameter {
            name: "id".to_string(),
            description: None,
            parameter_in: "path".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: Some(RefOr::T(Schema::Object(schema)).into()),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        assert!(format!("{err}").contains("Path parameter 'id' must set required: true"));
    }

    #[test]
    fn test_path_parameter_required_true_allows_processing() {
        let schema = ObjectBuilder::new().schema_type(Type::String).build();
        let param = ShimParameter {
            name: "id".to_string(),
            description: None,
            parameter_in: "path".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: Some(RefOr::T(Schema::Object(schema)).into()),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(processed.ty, "String");
    }

    #[test]
    fn test_resolve_swagger2_collection_format() {
        // Case: Query param with collectionFormat="ssv"
        // Expect: Style=SpaceDelimited
        let param = ShimParameter {
            name: "tags".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: Some("ssv".to_string()),
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, false).unwrap();
        assert_eq!(processed.style, Some(ParamStyle::SpaceDelimited));
    }

    #[test]
    fn test_resolve_explicit_overrides() {
        // Case: Header param with explicit explode=true
        let param = ShimParameter {
            name: "X-Ids".to_string(),
            description: None,
            parameter_in: "header".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None, // defaults to simple
            explode: Some(true),
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(processed.style, Some(ParamStyle::Simple));
        assert_eq!(processed.explode, true);
    }

    #[test]
    fn test_resolve_reusable_parameters() {
        // New structure requires generic components to be in 'extra' for legacy resolution.
        // ShimComponents handles this via flattening.
        let components_json = serde_json::json!({
            "parameters": {
                "limitParam": {
                    "name": "limit",
                    "in": "query",
                    "style": "form",
                    "explode": false,
                    "schema": { "type": "integer" }
                }
            }
        });

        let components: ShimComponents = serde_json::from_value(components_json).unwrap();

        let op_params = vec![RefOr::Ref(utoipa::openapi::Ref::new(
            "#/components/parameters/limitParam",
        ))];

        let resolved = resolve_parameters(&op_params, Some(&components), true).unwrap();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "limit");
        assert_eq!(resolved[0].style, Some(ParamStyle::Form));
        assert_eq!(resolved[0].explode, false);
    }

    #[test]
    fn test_resolve_parameter_ref_with_self() {
        let components_json = serde_json::json!({
            "__self": "https://example.com/openapi.yaml",
            "parameters": {
                "limitParam": {
                    "name": "limit",
                    "in": "query",
                    "schema": { "type": "integer" }
                }
            }
        });

        let components: ShimComponents = serde_json::from_value(components_json).unwrap();

        let op_params = vec![RefOr::Ref(utoipa::openapi::Ref::new(
            "https://example.com/openapi.yaml#/components/parameters/limitParam",
        ))];

        let resolved = resolve_parameters(&op_params, Some(&components), true).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "limit");
        assert_eq!(resolved[0].source, ParamSource::Query);
    }

    #[test]
    fn test_parameter_ref_description_override() {
        let components_json = serde_json::json!({
            "__self": "https://example.com/openapi.yaml",
            "parameters": {
                "limitParam": {
                    "name": "limit",
                    "description": "original",
                    "in": "query",
                    "schema": { "type": "integer" }
                }
            }
        });

        let components: ShimComponents = serde_json::from_value(components_json).unwrap();
        let mut ref_param = utoipa::openapi::Ref::new(
            "https://example.com/openapi.yaml#/components/parameters/limitParam",
        );
        ref_param.description = "override".to_string();

        let op_params = vec![RefOr::Ref(ref_param)];
        let resolved = resolve_parameters(&op_params, Some(&components), true).unwrap();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].description.as_deref(), Some("override"));
    }

    #[test]
    fn test_querystring_requires_content() {
        let param = ShimParameter {
            name: "raw".to_string(),
            description: None,
            parameter_in: "querystring".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: Some(
                RefOr::T(Schema::Object(
                    ObjectBuilder::new().schema_type(Type::Object).build(),
                ))
                .into(),
            ),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Querystring parameter 'raw' must use 'content'"));
    }

    #[test]
    fn test_parameter_content_single_entry_enforced() {
        let schema = ObjectBuilder::new().schema_type(Type::Object).build();
        let content_item = ContentBuilder::new()
            .schema(Some(RefOr::T(Schema::Object(schema))))
            .build();
        let mut content_map = BTreeMap::new();
        content_map.insert("application/json".into(), content_item.clone());
        content_map.insert("application/xml".into(), content_item);

        let param = ShimParameter {
            name: "filter".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: None,
            content: Some(content_map),
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("must define exactly one media type"));
    }

    #[test]
    fn test_schema_and_content_mutual_exclusive() {
        let schema = ObjectBuilder::new().schema_type(Type::String).build();
        let content_item = ContentBuilder::new()
            .schema(Some(RefOr::T(Schema::Object(schema))))
            .build();
        let mut content_map = BTreeMap::new();
        content_map.insert("text/plain".into(), content_item);

        let param = ShimParameter {
            name: "mix".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: Some(
                RefOr::T(Schema::Object(
                    ObjectBuilder::new().schema_type(Type::String).build(),
                ))
                .into(),
            ),
            content: Some(content_map),
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("cannot specify both 'schema' and 'content'"));
    }

    #[test]
    fn test_parameter_examples_pick_data_value() {
        let mut examples = BTreeMap::new();
        examples.insert(
            "sample".to_string(),
            serde_json::json!({ "dataValue": "hello" }),
        );

        let param = ShimParameter {
            name: "greet".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: Some(examples),
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(
            processed.example,
            Some(ExampleValue::data(serde_json::json!("hello")))
        );
    }

    #[test]
    fn test_parameter_examples_conflicting_fields_rejected() {
        let mut examples = BTreeMap::new();
        examples.insert(
            "bad".to_string(),
            serde_json::json!({
                "value": "hello",
                "serializedValue": "hello"
            }),
        );

        let param = ShimParameter {
            name: "greet".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: Some(examples),
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        assert!(format!("{err}").contains("examples.bad"));
    }

    #[test]
    fn test_parameter_examples_resolve_component_ref() {
        let components_json = serde_json::json!({
            "__self": "https://example.com/openapi.yaml",
            "examples": {
                "Greeting": {
                    "value": "hi"
                }
            }
        });
        let components: ShimComponents = serde_json::from_value(components_json).unwrap();

        let mut examples = BTreeMap::new();
        examples.insert(
            "greeting".to_string(),
            serde_json::json!({
                "$ref": "https://example.com/openapi.yaml#/components/examples/Greeting"
            }),
        );

        let param = ShimParameter {
            name: "salute".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: Some(examples),
            ..Default::default()
        };

        let processed = process_parameter(&param, Some(&components), true).unwrap();
        assert_eq!(
            processed.example,
            Some(ExampleValue::data(serde_json::json!("hi")))
        );
    }

    #[test]
    fn test_parameter_examples_ref_overrides_metadata() {
        let components_json = serde_json::json!({
            "__self": "https://example.com/openapi.yaml",
            "examples": {
                "Greeting": {
                    "summary": "Original summary",
                    "description": "Original description",
                    "value": "hi"
                }
            }
        });
        let components: ShimComponents = serde_json::from_value(components_json).unwrap();

        let mut examples = BTreeMap::new();
        examples.insert(
            "greeting".to_string(),
            serde_json::json!({
                "$ref": "https://example.com/openapi.yaml#/components/examples/Greeting",
                "summary": "Override summary",
                "description": "Override description"
            }),
        );

        let param = ShimParameter {
            name: "salute".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: Some(examples),
            ..Default::default()
        };

        let processed = process_parameter(&param, Some(&components), true).unwrap();
        assert_eq!(
            processed.example,
            Some(ExampleValue::data_with_meta(
                serde_json::json!("hi"),
                Some("Override summary".to_string()),
                Some("Override description".to_string())
            ))
        );
    }

    #[test]
    fn test_parameter_examples_pick_external_value() {
        let mut examples = BTreeMap::new();
        examples.insert(
            "external".to_string(),
            serde_json::json!({ "externalValue": "https://example.com/example.txt" }),
        );

        let param = ShimParameter {
            name: "doc".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: Some(examples),
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(
            processed.example,
            Some(ExampleValue::external(serde_json::json!(
                "https://example.com/example.txt"
            )))
        );
    }

    #[test]
    fn test_parameter_examples_pick_serialized_value() {
        let mut examples = BTreeMap::new();
        examples.insert(
            "serialized".to_string(),
            serde_json::json!({ "serializedValue": "color=blue%20black" }),
        );

        let param = ShimParameter {
            name: "color".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: Some(examples),
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(
            processed.example,
            Some(ExampleValue::serialized(serde_json::json!(
                "color=blue%20black"
            )))
        );
    }

    #[test]
    fn test_parameter_examples_conflicting_value_and_serialized_rejected() {
        let mut examples = BTreeMap::new();
        examples.insert(
            "serialized".to_string(),
            serde_json::json!({
                "value": "blue black",
                "serializedValue": "color=blue%20black"
            }),
        );

        let param = ShimParameter {
            name: "color".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: Some(examples),
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        assert!(format!("{err}").contains("must not define 'value'"));
    }

    #[test]
    fn test_parameter_schema_true_maps_to_string() {
        let param = ShimParameter {
            name: "any".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: Some(SchemaOrBool::Bool(true)),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(processed.ty, "String");
    }

    #[test]
    fn test_parameter_schema_false_rejected() {
        let param = ShimParameter {
            name: "never".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: Some(SchemaOrBool::Bool(false)),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        assert!(format!("{err}").contains("schema is 'false'"));
    }

    #[test]
    fn test_duplicate_parameters_rejected() {
        let param = ShimParameter {
            name: "limit".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let params = vec![RefOr::T(param.clone()), RefOr::T(param)];
        let err = resolve_parameters(&params, None, true).unwrap_err();
        assert!(format!("{err}").contains("Duplicate parameter"));
    }

    #[test]
    fn test_header_parameter_duplicates_case_insensitive() {
        let param_a = ShimParameter {
            name: "X-Token".to_string(),
            description: None,
            parameter_in: "header".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };
        let mut param_b = param_a.clone();
        param_b.name = "x-token".to_string();

        let params = vec![RefOr::T(param_a), RefOr::T(param_b)];
        let err = resolve_parameters(&params, None, true).unwrap_err();
        assert!(format!("{err}").contains("Duplicate parameter"));
    }

    #[test]
    fn test_reserved_header_parameters_ignored() {
        let reserved = vec![
            ShimParameter {
                name: "Accept".to_string(),
                description: None,
                parameter_in: "header".to_string(),
                schema_type: None,
                format: None,
                items: None,
                required: false,
                deprecated: false,
                schema: string_schema(),
                content: None,
                style: None,
                explode: None,
                allow_empty_value: None,
                allow_reserved: None,
                collection_format: None,
                example: None,
                examples: None,
                ..Default::default()
            },
            ShimParameter {
                name: "content-type".to_string(),
                description: None,
                parameter_in: "header".to_string(),
                schema_type: None,
                format: None,
                items: None,
                required: false,
                deprecated: false,
                schema: string_schema(),
                content: None,
                style: None,
                explode: None,
                allow_empty_value: None,
                allow_reserved: None,
                collection_format: None,
                example: None,
                examples: None,
                ..Default::default()
            },
            ShimParameter {
                name: "AUTHORIZATION".to_string(),
                description: None,
                parameter_in: "header".to_string(),
                schema_type: None,
                format: None,
                items: None,
                required: false,
                deprecated: false,
                schema: string_schema(),
                content: None,
                style: None,
                explode: None,
                allow_empty_value: None,
                allow_reserved: None,
                collection_format: None,
                example: None,
                examples: None,
                ..Default::default()
            },
        ];

        let custom = ShimParameter {
            name: "X-Custom".to_string(),
            description: None,
            parameter_in: "header".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: string_schema(),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let params = reserved
            .into_iter()
            .map(RefOr::T)
            .chain(std::iter::once(RefOr::T(custom)))
            .collect::<Vec<_>>();

        let resolved = resolve_parameters(&params, None, true).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "X-Custom");
        assert_eq!(resolved[0].source, ParamSource::Header);
    }

    #[test]
    fn test_parameter_content_rejects_style_fields() {
        let schema = ObjectBuilder::new().schema_type(Type::Object).build();
        let content_item = ContentBuilder::new()
            .schema(Some(RefOr::T(Schema::Object(schema))))
            .build();
        let mut content_map = BTreeMap::new();
        content_map.insert("application/json".into(), content_item);

        let param = ShimParameter {
            name: "payload".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: None,
            content: Some(content_map),
            style: Some("form".to_string()),
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        assert!(format!("{err}").contains("must not define style"));
    }

    #[test]
    #[allow(deprecated)]
    fn test_schema_example_fallback_inline() {
        let schema = Schema::Object(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .example(Some(json!("from-schema")))
                .build(),
        );

        let param = ShimParameter {
            name: "q".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: Some(RefOr::T(schema).into()),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(
            processed.example,
            Some(ExampleValue::data(json!("from-schema")))
        );
    }

    #[test]
    fn test_schema_example_fallback_ref() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "__self".to_string(),
            json!("https://example.com/openapi.yaml"),
        );
        components.extra.insert(
            "schemas".to_string(),
            json!({
                "Filter": {
                    "example": { "status": "active" }
                }
            }),
        );

        let param = ShimParameter {
            name: "filter".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: Some(
                RefOr::Ref(Ref::new(
                    "https://example.com/openapi.yaml#/components/schemas/Filter",
                ))
                .into(),
            ),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, Some(&components), true).unwrap();
        assert_eq!(
            processed.example,
            Some(ExampleValue::data(json!({ "status": "active" })))
        );
    }

    #[test]
    fn test_allow_empty_value_query_only() {
        let param = ShimParameter {
            name: "q".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: Some(
                RefOr::T(Schema::Object(
                    ObjectBuilder::new().schema_type(Type::String).build(),
                ))
                .into(),
            ),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: Some(true),
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert!(processed.allow_empty_value);
    }

    #[test]
    fn test_allow_empty_value_rejects_non_query() {
        let param = ShimParameter {
            name: "id".to_string(),
            description: None,
            parameter_in: "path".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: Some(
                RefOr::T(Schema::Object(
                    ObjectBuilder::new().schema_type(Type::String).build(),
                ))
                .into(),
            ),
            content: None,
            style: None,
            explode: None,
            allow_empty_value: Some(true),
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            ..Default::default()
        };

        let err = process_parameter(&param, None, true).unwrap_err();
        assert!(format!("{err}").contains("allowEmptyValue"));
    }

    #[test]
    fn test_parameter_content_media_type_ref_schema() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "__self".to_string(),
            serde_json::json!("https://example.com/openapi.yaml"),
        );
        components.extra.insert(
            "mediaTypes".to_string(),
            serde_json::json!({
                "QueryFilter": {
                    "schema": { "type": "integer", "format": "int32" }
                }
            }),
        );

        let mut content_map = BTreeMap::new();
        content_map.insert("application/json".into(), ContentBuilder::new().build());

        let raw = serde_json::json!({
            "name": "filter",
            "in": "query",
            "content": {
                "application/json": {
                    "$ref": "#/components/mediaTypes/QueryFilter"
                }
            }
        });

        let param = ShimParameter {
            name: "filter".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: true,
            deprecated: false,
            schema: None,
            content: Some(content_map),
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            raw,
            ..Default::default()
        };

        let processed = process_parameter(&param, Some(&components), true).unwrap();
        assert_eq!(processed.ty, "i32");
        assert_eq!(processed.content_media_type, Some(ContentMediaType::Json));
    }

    #[test]
    fn test_parameter_content_item_schema_sequential() {
        let mut content_map = BTreeMap::new();
        content_map.insert("application/x-ndjson".into(), ContentBuilder::new().build());

        let raw = serde_json::json!({
            "name": "events",
            "in": "query",
            "content": {
                "application/x-ndjson": {
                    "itemSchema": { "type": "string" }
                }
            }
        });

        let param = ShimParameter {
            name: "events".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: None,
            content: Some(content_map),
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            raw,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(processed.ty, "Vec<String>");
    }

    #[test]
    fn test_parameter_content_item_schema_vendor_jsonl() {
        let mut content_map = BTreeMap::new();
        content_map.insert(
            "application/vnd.acme+jsonl".into(),
            ContentBuilder::new().build(),
        );

        let raw = serde_json::json!({
            "name": "events",
            "in": "query",
            "content": {
                "application/vnd.acme+jsonl": {
                    "itemSchema": { "type": "string" }
                }
            }
        });

        let param = ShimParameter {
            name: "events".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: None,
            content: Some(content_map),
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            raw,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(processed.ty, "Vec<String>");
    }

    #[test]
    fn test_parameter_content_examples_serialized_value() {
        let mut content_map = BTreeMap::new();
        content_map.insert("application/json".into(), ContentBuilder::new().build());

        let raw = serde_json::json!({
            "name": "payload",
            "in": "query",
            "content": {
                "application/json": {
                    "examples": {
                        "sample": {
                            "serializedValue": "{\"id\":1}"
                        }
                    }
                }
            }
        });

        let param = ShimParameter {
            name: "payload".to_string(),
            description: None,
            parameter_in: "query".to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: None,
            content: Some(content_map),
            style: None,
            explode: None,
            allow_empty_value: None,
            allow_reserved: None,
            collection_format: None,
            example: None,
            examples: None,
            raw,
            ..Default::default()
        };

        let processed = process_parameter(&param, None, true).unwrap();
        assert_eq!(
            processed.example,
            Some(ExampleValue::serialized(serde_json::json!("{\"id\":1}")))
        );
    }
}
