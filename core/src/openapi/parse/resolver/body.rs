#![deny(missing_docs)]

//! # Body Resolution
//!
//! Logic for extracting request body types from OpenAPI definitions.
//! Support for OAS 3.2 `encoding` definitions in multipart and url-encoded forms,
//! `components.mediaTypes` `$ref` resolution inside `content`, plus detection of vendor JSON,
//! sequential media types, XML, text, and binary media types.
//!
//! Boolean schemas (`schema: true/false`) are handled by treating `true` as an unconstrained
//! schema and rejecting `false` when the request body is required.

use crate::error::{AppError, AppResult};
use crate::openapi::parse::models::{
    BodyFormat, EncodingInfo, ExampleValue, ParamStyle, RequestBodyDefinition,
};
use crate::openapi::parse::ref_utils::extract_component_name;
use crate::openapi::parse::registry::DocumentRegistry;
use crate::openapi::parse::resolver::types::{
    map_schema_to_rust_type, map_schema_to_rust_type_with_raw,
};
use crate::openapi::parse::routes::shims::{ShimComponents, ShimRequestBody};
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use url::Url;
use utoipa::openapi::encoding::Encoding;
use utoipa::openapi::example::Example;
use utoipa::openapi::path::ParameterStyle;
use utoipa::openapi::{schema::Schema, RefOr, Required};

struct ResolvedRequestBody {
    body: ShimRequestBody,
    components: Option<ShimComponents>,
    base_uri: Option<Url>,
}

/// Extracts the request body type and format from the OpenAPI definition.
///
/// Resolves `$ref` values against `components.requestBodies` when available, including
/// OAS 3.2 `$self`-qualified absolute references.
pub fn extract_request_body_type(
    body: &RefOr<ShimRequestBody>,
    components: Option<&ShimComponents>,
) -> AppResult<Option<RequestBodyDefinition>> {
    extract_request_body_type_with_registry(body, components, None, None)
}

/// Extracts the request body type and format, allowing external reference resolution.
pub fn extract_request_body_type_with_registry(
    body: &RefOr<ShimRequestBody>,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<Option<RequestBodyDefinition>> {
    let mut owned_body: Option<ShimRequestBody> = None;
    let _ = owned_body.as_ref();
    let mut override_components: Option<ShimComponents> = None;
    let mut override_base: Option<Url> = None;
    let (content, required, description, raw_body) = match body {
        RefOr::T(b) => (
            &b.inner.content,
            is_body_required(b),
            b.inner.description.clone(),
            Some(&b.raw),
        ),
        RefOr::Ref(r) => {
            if let Some(mut resolved) =
                resolve_request_body_from_components(r, components, registry, base_uri)
            {
                if !r.description.is_empty() {
                    resolved.body.inner.description = Some(r.description.clone());
                }
                override_components = resolved.components.take();
                override_base = resolved.base_uri.take();
                let required = is_body_required(&resolved.body);
                let description = resolved.body.inner.description.clone();
                owned_body = Some(resolved.body);
                let owned = owned_body.as_ref().expect("owned body");
                (
                    &owned.inner.content,
                    required,
                    description,
                    Some(&owned.raw),
                )
            } else {
                return Ok(None);
            }
        }
    };

    let components_ctx = override_components.as_ref().or(components);
    let base_ctx = override_base.as_ref().or(base_uri);

    let Some((format, media_type, media)) = select_request_media(content) else {
        return Ok(None);
    };

    let raw_media = raw_body
        .and_then(|raw| raw_media_for_type(raw, media_type, components_ctx, registry, base_ctx));
    if raw_schema_is_false(raw_media.as_ref(), "schema") {
        if required {
            return Err(AppError::General(
                "RequestBody schema is 'false' but the request body is required".to_string(),
            ));
        }
        return Ok(None);
    }
    if raw_schema_is_false(raw_media.as_ref(), "itemSchema") {
        if required {
            return Err(AppError::General(
                "RequestBody itemSchema is 'false' but the request body is required".to_string(),
            ));
        }
        return Ok(None);
    }
    let raw_schema_json = raw_media.as_ref().and_then(|raw| raw.get("schema"));
    let raw_schema = raw_media.as_ref().and_then(extract_media_schema);
    let item_schema = raw_media.as_ref().and_then(extract_item_schema);

    let ty = if let Some(schema_ref) = &media.schema {
        map_schema_to_rust_type_with_raw(schema_ref, true, raw_schema_json)?
    } else if let Some(schema_ref) = raw_schema.as_ref() {
        map_schema_to_rust_type_with_raw(schema_ref, true, raw_schema_json)?
    } else if let Some(item_schema) = item_schema {
        let inner = map_schema_to_rust_type(&RefOr::T(item_schema), true)?;
        if is_sequential_media_type(media_type) {
            format!("Vec<{}>", inner)
        } else {
            inner
        }
    } else {
        default_body_type(format, media_type)
    };

    let encoding = match format {
        BodyFormat::Form | BodyFormat::Multipart => extract_encoding_map_with_raw(
            &media.encoding,
            raw_media.as_ref(),
            components_ctx,
            registry,
            base_ctx,
        )?,
        _ => None,
    };
    let (prefix_encoding, item_encoding) = match format {
        BodyFormat::Multipart => {
            extract_positional_encoding(raw_media.as_ref(), components_ctx, registry, base_ctx)?
        }
        _ => (None, None),
    };
    if encoding.is_some() && (prefix_encoding.is_some() || item_encoding.is_some()) {
        return Err(AppError::General(
            "MediaType cannot define both 'encoding' and positional 'prefixEncoding'/'itemEncoding'" 
                .to_string(),
        ));
    }
    let example = extract_media_example(
        media,
        components_ctx,
        registry,
        base_ctx,
        raw_media.as_ref(),
        media_type,
    );

    Ok(Some(RequestBodyDefinition {
        ty,
        description,
        media_type: media_type.to_string(),
        format,
        required,
        encoding,
        prefix_encoding,
        item_encoding,
        example,
    }))
}

/// Extracts the raw request body payload for round-trip preservation.
///
/// Returns the full `requestBody` object (including all media types and examples)
/// when it can be resolved from either inline content or `components.requestBodies`.
pub fn extract_request_body_raw(
    body: &RefOr<ShimRequestBody>,
    components: Option<&ShimComponents>,
) -> Option<serde_json::Value> {
    extract_request_body_raw_with_registry(body, components, None, None)
}

/// Extracts raw request bodies with external reference resolution.
pub fn extract_request_body_raw_with_registry(
    body: &RefOr<ShimRequestBody>,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<serde_json::Value> {
    match body {
        RefOr::T(b) => Some(b.raw.clone()),
        RefOr::Ref(r) => resolve_request_body_from_components(r, components, registry, base_uri)
            .map(|b| b.body.raw),
    }
}

fn is_body_required(body: &ShimRequestBody) -> bool {
    matches!(body.inner.required, Some(Required::True))
}

fn select_request_media(
    content: &BTreeMap<String, utoipa::openapi::Content>,
) -> Option<(BodyFormat, &str, &utoipa::openapi::Content)> {
    // 1. JSON (application/json or +json)
    if let Some((key, media)) =
        select_best_media(content, is_json_media_type, Some("application/json"))
    {
        return Some((BodyFormat::Json, key, media));
    }

    // 2. Form URL Encoded
    if let Some((key, media)) = select_best_media(
        content,
        is_form_media_type,
        Some("application/x-www-form-urlencoded"),
    ) {
        return Some((BodyFormat::Form, key, media));
    }

    // 3. Multipart
    if let Some((key, media)) = select_best_media(
        content,
        is_multipart_media_type,
        Some("multipart/form-data"),
    ) {
        return Some((BodyFormat::Multipart, key, media));
    }

    // 4. Text
    if let Some((key, media)) = select_best_media(content, is_text_media_type, Some("text/plain")) {
        return Some((BodyFormat::Text, key, media));
    }

    // 5. Binary
    if let Some((key, media)) = select_best_media(
        content,
        is_binary_media_type,
        Some("application/octet-stream"),
    ) {
        return Some((BodyFormat::Binary, key, media));
    }

    // 6. Wildcards: application/* and */*
    if let Some((key, media)) = select_best_media(
        content,
        |k| normalize_media_type(k) == "application/*" || normalize_media_type(k) == "*/*",
        Some("application/*"),
    ) {
        return Some((BodyFormat::Binary, key, media));
    }

    // 7. Fallback: take the first media type as binary.
    content
        .iter()
        .next()
        .map(|(k, v)| (BodyFormat::Binary, k.as_str(), v))
}

fn select_best_media<'a, F>(
    content: &'a BTreeMap<String, utoipa::openapi::Content>,
    predicate: F,
    preferred: Option<&str>,
) -> Option<(&'a str, &'a utoipa::openapi::Content)>
where
    F: Fn(&str) -> bool,
{
    content
        .iter()
        .filter(|(k, _)| predicate(k))
        .max_by_key(|(k, _)| media_specificity_score(k, preferred))
        .map(|(k, v)| (k.as_str(), v))
}

fn media_specificity_score(media_type: &str, preferred: Option<&str>) -> (i32, i32, usize) {
    let normalized = normalize_media_type(media_type);
    let preferred_bonus = preferred
        .map(|p| normalize_media_type(p) == normalized)
        .unwrap_or(false) as i32;
    let wildcard_score = if normalized == "*/*" {
        0
    } else if normalized.contains('*') {
        1
    } else {
        2
    };
    (preferred_bonus, wildcard_score, normalized.len())
}

fn is_json_media_type(media_type: &str) -> bool {
    crate::openapi::parse::models::MediaType::parse(media_type).is_json()
}

fn is_form_media_type(media_type: &str) -> bool {
    crate::openapi::parse::models::MediaType::parse(media_type).is_form_urlencoded()
}

fn is_multipart_media_type(media_type: &str) -> bool {
    crate::openapi::parse::models::MediaType::parse(media_type).is_multipart()
}

fn is_text_media_type(media_type: &str) -> bool {
    crate::openapi::parse::models::MediaType::parse(media_type).is_text()
}

fn is_binary_media_type(media_type: &str) -> bool {
    let normalized = normalize_media_type(media_type);
    normalized == "application/octet-stream"
        || normalized == "application/pdf"
        || normalized.starts_with("image/")
        || normalized.starts_with("audio/")
        || normalized.starts_with("video/")
}

fn normalize_media_type(media_type: &str) -> String {
    crate::openapi::parse::models::MediaType::normalize(media_type)
}

fn is_sequential_json_media_type(media_type: &str) -> bool {
    crate::openapi::parse::models::MediaType::parse(media_type).is_sequential_json()
}

fn is_sequential_media_type(media_type: &str) -> bool {
    let normalized = normalize_media_type(media_type);
    if is_sequential_json_media_type(&normalized) {
        return true;
    }

    matches!(
        normalized.as_str(),
        "text/event-stream" | "multipart/mixed" | "multipart/byteranges"
    )
}

fn default_body_type(format: BodyFormat, media_type: &str) -> String {
    match format {
        BodyFormat::Json => {
            if is_sequential_json_media_type(media_type) {
                "Vec<serde_json::Value>".to_string()
            } else {
                "serde_json::Value".to_string()
            }
        }
        BodyFormat::Form => "serde_json::Value".to_string(),
        BodyFormat::Multipart => "Multipart".to_string(),
        BodyFormat::Text => "String".to_string(),
        BodyFormat::Binary => infer_binary_type(media_type),
    }
}

fn infer_binary_type(media_type: &str) -> String {
    let normalized = normalize_media_type(media_type);
    if normalized.starts_with("text/") {
        return "String".to_string();
    }

    "Vec<u8>".to_string()
}

fn resolve_request_body_from_components(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<ResolvedRequestBody> {
    if let Some(body) = components
        .and_then(|c| {
            let name =
                extract_component_name(&r.ref_location, c.self_uri.as_deref(), "requestBodies")?;
            c.extra.get("requestBodies")?.get(&name)
        })
        .and_then(|v| serde_json::from_value::<ShimRequestBody>(v.clone()).ok())
    {
        return Some(ResolvedRequestBody {
            body,
            components: None,
            base_uri: None,
        });
    }

    registry
        .and_then(|reg| {
            reg.resolve_component_ref_with_components(&r.ref_location, base_uri, "requestBodies")
        })
        .and_then(|(raw, comps_override, base_override)| {
            let body = serde_json::from_value::<ShimRequestBody>(raw).ok()?;
            Some(ResolvedRequestBody {
                body,
                components: comps_override,
                base_uri: base_override,
            })
        })
}

/// Extracts a single example value for a media type, respecting OAS 3.2 example fields.
pub(crate) fn extract_media_example(
    media: &utoipa::openapi::Content,
    components: Option<&ShimComponents>,
    _registry: Option<&DocumentRegistry>,
    _base_uri: Option<&Url>,
    raw_media: Option<&serde_json::Value>,
    media_type: &str,
) -> Option<ExampleValue> {
    if let Some(raw) = raw_media {
        if let Some(example) = raw.get("example") {
            return Some(ExampleValue::data(example.clone()));
        }

        if let Some(examples) = raw.get("examples").and_then(|v| v.as_object()) {
            let mut visiting = std::collections::HashSet::new();
            for value in examples.values() {
                if let Some(val) =
                    extract_example_value(value, components, media_type, &mut visiting)
                {
                    return Some(val);
                }
            }
        }
    }

    if let Some(example) = &media.example {
        return Some(ExampleValue::data(example.clone()));
    }

    for example_ref in media.examples.values() {
        if let Some(value) = extract_example_from_ref_or(example_ref, components) {
            return Some(value);
        }
    }

    if let Some(schema_ref) = &media.schema {
        if let Some(example) = extract_schema_example(schema_ref, components) {
            return Some(ExampleValue::data(example));
        }
    }

    None
}

fn extract_example_from_utoipa(example: &Example) -> Option<ExampleValue> {
    let summary = (!example.summary.is_empty()).then(|| example.summary.clone());
    let description = (!example.description.is_empty()).then(|| example.description.clone());
    if let Some(val) = example.value.clone() {
        return Some(ExampleValue::data_with_meta(val, summary, description));
    }
    if !example.external_value.is_empty() {
        return Some(ExampleValue::external_with_meta(
            json!(example.external_value.clone()),
            summary,
            description,
        ));
    }
    None
}

fn extract_example_from_ref_or(
    example_ref: &RefOr<Example>,
    components: Option<&ShimComponents>,
) -> Option<ExampleValue> {
    match example_ref {
        RefOr::T(example) => extract_example_from_utoipa(example),
        RefOr::Ref(r) => {
            let summary = (!r.summary.is_empty()).then(|| r.summary.clone());
            let description = (!r.description.is_empty()).then(|| r.description.clone());
            resolve_example_from_components(r, components)
                .map(|example| example.with_overrides(summary, description))
        }
    }
}

/// Extracts an Example Object value (data/serialized/external), resolving `$ref` when present.
pub(crate) fn extract_example_value(
    value: &serde_json::Value,
    components: Option<&ShimComponents>,
    media_type: &str,
    visiting: &mut std::collections::HashSet<String>,
) -> Option<ExampleValue> {
    if let Some(obj) = value.as_object() {
        let (summary, description) = example_meta_from_obj(obj);
        if let Some(ref_str) = obj.get("$ref").and_then(|v| v.as_str()) {
            return resolve_example_ref(ref_str, components, media_type, visiting)
                .map(|example| example.with_overrides(summary, description));
        }
        if let Some(val) = obj.get("dataValue") {
            return Some(ExampleValue::data_with_meta(
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
        if let Some(val) = obj.get("serializedValue") {
            if let Some(serialized) = val.as_str() {
                if is_json_media_type(media_type) {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(serialized) {
                        return Some(ExampleValue::data_with_meta(parsed, summary, description));
                    }
                }
            }
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
        return None;
    }

    if !value.is_null() {
        return Some(ExampleValue::data(value.clone()));
    }

    None
}

fn resolve_example_ref(
    ref_str: &str,
    components: Option<&ShimComponents>,
    media_type: &str,
    visiting: &mut std::collections::HashSet<String>,
) -> Option<ExampleValue> {
    if !visiting.insert(ref_str.to_string()) {
        return None;
    }

    let (comps, self_uri) = components.map(|c| (c, c.self_uri.as_deref()))?;
    let name = extract_component_name(ref_str, self_uri, "examples")?;
    let example_json = comps.extra.get("examples").and_then(|e| e.get(&name))?;
    extract_example_value(example_json, components, media_type, visiting)
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

fn resolve_example_from_components(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
) -> Option<ExampleValue> {
    let (comps, self_uri) = components.map(|c| (c, c.self_uri.as_deref()))?;
    let ref_name = extract_component_name(&r.ref_location, self_uri, "examples")?;
    let example_json = comps.extra.get("examples").and_then(|e| e.get(&ref_name))?;
    let example = serde_json::from_value::<Example>(example_json.clone()).ok()?;
    extract_example_from_utoipa(&example)
}

fn extract_schema_example(
    schema_ref: &RefOr<Schema>,
    components: Option<&ShimComponents>,
) -> Option<serde_json::Value> {
    match schema_ref {
        RefOr::T(schema) => extract_example_from_schema(schema),
        RefOr::Ref(r) => resolve_schema_example_ref(&r.ref_location, components),
    }
}

fn extract_example_from_schema(schema: &Schema) -> Option<serde_json::Value> {
    let (example, examples) = match schema {
        Schema::Object(obj) => (&obj.example, &obj.examples),
        Schema::Array(arr) => (&arr.example, &arr.examples),
        Schema::OneOf(one_of) => (&one_of.example, &one_of.examples),
        Schema::AnyOf(any_of) => (&any_of.example, &any_of.examples),
        _ => return None,
    };
    example.clone().or_else(|| examples.first().cloned())
}

fn resolve_schema_example_ref(
    ref_str: &str,
    components: Option<&ShimComponents>,
) -> Option<serde_json::Value> {
    let (comps, self_uri) = components.map(|c| (c, c.self_uri.as_deref()))?;
    let name = extract_component_name(ref_str, self_uri, "schemas")?;
    let schema_json = comps.extra.get("schemas").and_then(|s| s.get(&name))?;
    extract_schema_example_from_value(schema_json)
}

fn raw_media_for_type(
    raw_body: &serde_json::Value,
    media_type: &str,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<serde_json::Value> {
    let media = raw_body.get("content")?.get(media_type)?;
    resolve_media_type_ref(media, components, registry, base_uri, &mut HashSet::new())
        .or_else(|| media.as_object().map(|_| media.clone()))
}

fn resolve_media_type_ref(
    raw_media: &serde_json::Value,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
    visiting: &mut HashSet<String>,
) -> Option<serde_json::Value> {
    let ref_str = raw_media.as_object()?.get("$ref")?.as_str()?;
    if !visiting.insert(ref_str.to_string()) {
        return None;
    }

    if let Some(resolved) = components.and_then(|c| {
        let name = extract_component_name(ref_str, c.self_uri.as_deref(), "mediaTypes")?;
        c.extra.get("mediaTypes")?.as_object()?.get(&name)
    }) {
        let value = resolve_media_type_ref(resolved, components, registry, base_uri, visiting)
            .unwrap_or_else(|| resolved.clone());
        visiting.remove(ref_str);
        return Some(value);
    }

    if let Some((resolved, comps_override, base_override)) = registry
        .and_then(|r| r.resolve_component_ref_with_components(ref_str, base_uri, "mediaTypes"))
    {
        let next_components = comps_override.as_ref().or(components);
        let next_base = base_override.as_ref().or(base_uri);
        let value =
            resolve_media_type_ref(&resolved, next_components, registry, next_base, visiting)
                .unwrap_or_else(|| resolved.clone());
        visiting.remove(ref_str);
        return Some(value);
    }

    visiting.remove(ref_str);
    None
}

fn extract_item_schema(raw_media: &serde_json::Value) -> Option<Schema> {
    let item_schema = raw_media.get("itemSchema")?;
    serde_json::from_value::<Schema>(item_schema.clone()).ok()
}

fn extract_media_schema(raw_media: &serde_json::Value) -> Option<RefOr<Schema>> {
    let schema_val = raw_media.get("schema")?;
    serde_json::from_value::<RefOr<Schema>>(schema_val.clone()).ok()
}

fn raw_schema_is_false(raw_media: Option<&serde_json::Value>, key: &str) -> bool {
    matches!(
        raw_media
            .and_then(|raw| raw.get(key))
            .and_then(|v| v.as_bool()),
        Some(false)
    )
}

fn extract_schema_example_from_value(value: &serde_json::Value) -> Option<serde_json::Value> {
    let obj = value.as_object()?;
    if let Some(example) = obj.get("example") {
        return Some(example.clone());
    }
    obj.get("examples")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first().cloned())
}

fn extract_positional_encoding(
    raw_media: Option<&serde_json::Value>,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<(Option<Vec<EncodingInfo>>, Option<EncodingInfo>)> {
    let Some(raw) = raw_media else {
        return Ok((None, None));
    };

    let mut prefix_encoding = None;
    if let Some(prefix_val) = raw.get("prefixEncoding") {
        let items = prefix_val.as_array().ok_or_else(|| {
            AppError::General("MediaType.prefixEncoding must be an array".to_string())
        })?;
        let mut parsed = Vec::new();
        for item in items {
            parsed.push(parse_encoding_value(item, components, registry, base_uri)?);
        }
        if !parsed.is_empty() {
            prefix_encoding = Some(parsed);
        }
    }

    let item_encoding = if let Some(item_val) = raw.get("itemEncoding") {
        Some(parse_encoding_value(
            item_val, components, registry, base_uri,
        )?)
    } else {
        None
    };

    Ok((prefix_encoding, item_encoding))
}

fn parse_encoding_value(
    value: &serde_json::Value,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<EncodingInfo> {
    let mut normalized = value.clone();
    if let serde_json::Value::Object(obj) = &mut normalized {
        obj.insert(
            "headers".to_string(),
            serde_json::Value::Object(serde_json::Map::new()),
        );
    }
    let enc = serde_json::from_value::<Encoding>(normalized).map_err(|e| {
        AppError::General(format!(
            "Failed to parse Encoding object from MediaType positional encoding: {}",
            e
        ))
    })?;

    let mut info = encoding_to_info(&enc)?;
    let obj = value.as_object().expect("Encoding must be an object");

    if let Some(headers_obj) = obj.get("headers").and_then(|h| h.as_object()) {
        let mut resolved_headers = HashMap::new();
        for (name, header_val) in headers_obj {
            if name.eq_ignore_ascii_case("content-type") {
                continue;
            }
            let ty = resolve_encoding_header_type(header_val, components, registry, base_uri)?;
            resolved_headers.insert(name.clone(), ty);
        }
        if !resolved_headers.is_empty() {
            info.headers = resolved_headers;
        }
    }

    if let Some(nested) = obj.get("encoding").and_then(|v| v.as_object()) {
        let mut nested_map = HashMap::new();
        for (prop, enc_val) in nested {
            nested_map.insert(
                prop.clone(),
                parse_encoding_value(enc_val, components, registry, base_uri)?,
            );
        }
        if !nested_map.is_empty() {
            info.encoding = Some(nested_map);
        }
    }

    if let Some(prefix_val) = obj.get("prefixEncoding").and_then(|v| v.as_array()) {
        let mut parsed = Vec::new();
        for item in prefix_val {
            parsed.push(parse_encoding_value(item, components, registry, base_uri)?);
        }
        if !parsed.is_empty() {
            info.prefix_encoding = Some(parsed);
        }
    }

    if let Some(item_val) = obj.get("itemEncoding") {
        let parsed = parse_encoding_value(item_val, components, registry, base_uri)?;
        info.item_encoding = Some(Box::new(parsed));
    }

    Ok(info)
}

fn extract_encoding_map_with_raw(
    encoding: &std::collections::BTreeMap<String, Encoding>,
    raw_media: Option<&serde_json::Value>,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<Option<HashMap<String, EncodingInfo>>> {
    if let Some(raw_encoding) = raw_media
        .and_then(|raw| raw.get("encoding"))
        .and_then(|value| value.as_object())
    {
        let mut map = HashMap::new();
        for (prop, enc_val) in raw_encoding {
            map.insert(
                prop.clone(),
                parse_encoding_value(enc_val, components, registry, base_uri)?,
            );
        }
        return Ok((!map.is_empty()).then_some(map));
    }

    if encoding.is_empty() {
        return Ok(None);
    }

    let mut map = HashMap::new();
    for (prop, enc) in encoding {
        map.insert(prop.clone(), encoding_to_info(enc)?);
    }

    Ok(Some(map))
}

fn resolve_encoding_header_type(
    header_val: &serde_json::Value,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<String> {
    let mut visited = HashSet::new();
    let resolved = resolve_header_value_for_encoding(
        header_val,
        components,
        registry,
        base_uri,
        &mut visited,
    )?;
    let Some(obj) = resolved.as_object() else {
        return Ok("String".to_string());
    };

    if let Some(schema_val) = obj.get("schema") {
        if let Some(flag) = schema_val.as_bool() {
            if !flag {
                return Err(AppError::General(
                    "Encoding header schema is 'false' and cannot be satisfied".to_string(),
                ));
            }
            return Ok("String".to_string());
        }
        let schema: RefOr<Schema> = serde_json::from_value(schema_val.clone()).map_err(|e| {
            AppError::General(format!("Failed to parse encoding header schema: {}", e))
        })?;
        return map_schema_to_rust_type(&schema, true);
    }

    if let Some(content_val) = obj.get("content") {
        let Some(content_map) = content_val.as_object() else {
            return Err(AppError::General(
                "Encoding header content must be an object".to_string(),
            ));
        };
        if content_map.len() != 1 {
            return Err(AppError::General(
                "Encoding header content must define exactly one media type".to_string(),
            ));
        }
        let (media_type, media_obj) = content_map.iter().next().expect("checked len == 1");
        let resolved_media = resolve_media_type_ref(
            media_obj,
            components,
            registry,
            base_uri,
            &mut HashSet::new(),
        )
        .unwrap_or_else(|| media_obj.clone());
        let Some(resolved_obj) = resolved_media.as_object() else {
            return Err(AppError::General(
                "Encoding header content must be an object".to_string(),
            ));
        };
        if let Some(schema_val) = resolved_obj.get("schema") {
            let schema: RefOr<Schema> =
                serde_json::from_value(schema_val.clone()).map_err(|e| {
                    AppError::General(format!(
                        "Failed to parse encoding header content schema: {}",
                        e
                    ))
                })?;
            return map_schema_to_rust_type(&schema, true);
        }
        if let Some(item_schema) = resolved_obj.get("itemSchema") {
            let schema: RefOr<Schema> =
                serde_json::from_value(item_schema.clone()).map_err(|e| {
                    AppError::General(format!("Failed to parse encoding header itemSchema: {}", e))
                })?;
            let inner = map_schema_to_rust_type(&schema, true)?;
            let ty = if is_sequential_media_type(&normalize_media_type(media_type)) {
                format!("Vec<{}>", inner)
            } else {
                inner
            };
            return Ok(ty);
        }

        return Ok("String".to_string());
    }

    Ok("String".to_string())
}

fn resolve_header_value_for_encoding(
    value: &serde_json::Value,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
    visited: &mut HashSet<String>,
) -> AppResult<serde_json::Value> {
    let Some(ref_str) = value
        .as_object()
        .and_then(|obj| obj.get("$ref"))
        .and_then(|v| v.as_str())
    else {
        return Ok(value.clone());
    };

    if !visited.insert(ref_str.to_string()) {
        return Err(AppError::General(format!(
            "Encoding header reference cycle detected at '{}'",
            ref_str
        )));
    }

    if let Some(resolved) = components.and_then(|c| {
        let name = extract_component_name(ref_str, c.self_uri.as_deref(), "headers")?;
        c.extra.get("headers")?.get(&name).cloned()
    }) {
        let result =
            resolve_header_value_for_encoding(&resolved, components, registry, base_uri, visited)?;
        visited.remove(ref_str);
        return Ok(result);
    }

    if let Some((resolved, comps_override, base_override)) =
        registry.and_then(|r| r.resolve_component_ref_with_components(ref_str, base_uri, "headers"))
    {
        let next_components = comps_override.as_ref().or(components);
        let next_base = base_override.as_ref().or(base_uri);
        let result = resolve_header_value_for_encoding(
            &resolved,
            next_components,
            registry,
            next_base,
            visited,
        )?;
        visited.remove(ref_str);
        return Ok(result);
    }

    Err(AppError::General(format!(
        "Encoding header reference '{}' not found",
        ref_str
    )))
}

fn encoding_to_info(enc: &Encoding) -> AppResult<EncodingInfo> {
    let mut headers = HashMap::new();
    for (h_name, h_ref) in &enc.headers {
        if h_name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        let ty = map_schema_to_rust_type(&h_ref.schema, true)?;
        headers.insert(h_name.clone(), ty);
    }

    Ok(EncodingInfo {
        content_type: enc.content_type.clone(),
        headers,
        style: map_param_style(enc.style.as_ref()),
        explode: enc.explode,
        allow_reserved: enc.allow_reserved,
        encoding: None,
        prefix_encoding: None,
        item_encoding: None,
    })
}

fn map_param_style(style: Option<&ParameterStyle>) -> Option<ParamStyle> {
    match style {
        Some(ParameterStyle::Matrix) => Some(ParamStyle::Matrix),
        Some(ParameterStyle::Label) => Some(ParamStyle::Label),
        Some(ParameterStyle::Form) => Some(ParamStyle::Form),
        Some(ParameterStyle::Simple) => Some(ParamStyle::Simple),
        Some(ParameterStyle::SpaceDelimited) => Some(ParamStyle::SpaceDelimited),
        Some(ParameterStyle::PipeDelimited) => Some(ParamStyle::PipeDelimited),
        Some(ParameterStyle::DeepObject) => Some(ParamStyle::DeepObject),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::ExampleValue;
    use crate::openapi::parse::routes::shims::ShimComponents;
    use crate::parse_openapi_routes;
    use serde_json::json;
    use std::collections::BTreeMap;
    use utoipa::openapi::encoding::EncodingBuilder;
    use utoipa::openapi::header::HeaderBuilder;
    use utoipa::openapi::path::ParameterStyle;
    use utoipa::openapi::request_body::RequestBodyBuilder;
    use utoipa::openapi::Content;
    use utoipa::openapi::Ref;

    #[test]
    fn test_extract_json_body() {
        let body = RequestBodyBuilder::new()
            .content(
                "application/json",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/User",
                )))),
            )
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.ty, "User");
        assert_eq!(def.media_type, "application/json");
        assert_eq!(def.format, BodyFormat::Json);
        assert!(!def.required);
        assert!(def.encoding.is_none());
    }

    #[test]
    fn test_request_body_prefers_specific_text_media() {
        let body = RequestBodyBuilder::new()
            .content("text/*", Content::new::<RefOr<Schema>>(None))
            .content("text/plain", Content::new::<RefOr<Schema>>(None))
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.media_type, "text/plain");
        assert_eq!(def.format, BodyFormat::Text);
    }

    #[test]
    fn test_request_body_prefers_application_json() {
        let body = RequestBodyBuilder::new()
            .content(
                "application/vnd.api+json",
                Content::new::<RefOr<Schema>>(None),
            )
            .content("application/json", Content::new::<RefOr<Schema>>(None))
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.media_type, "application/json");
        assert_eq!(def.format, BodyFormat::Json);
    }

    #[test]
    fn test_extract_request_body_content_schema_mapping() {
        let raw = json!({
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

        let shim: ShimRequestBody =
            serde_json::from_value(raw).expect("Failed to parse from value");
        let def = extract_request_body_type(&RefOr::T(shim), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.ty, "i32");
        assert_eq!(def.media_type, "application/json");
    }

    #[test]
    #[allow(deprecated)]
    fn test_extract_request_body_example_value() {
        let media = Content::builder()
            .schema(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                "#/components/schemas/User",
            ))))
            .example(Some(json!({"id": 1, "name": "Ada"})))
            .build();

        let body = RequestBodyBuilder::new()
            .content("application/json", media)
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(
            def.example,
            Some(ExampleValue::data(json!({"id": 1, "name": "Ada"})))
        );
    }

    #[test]
    fn test_extract_request_body_example_ref() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            self_uri: Some("https://example.com/openapi.yaml".to_string()),
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "examples".to_string(),
            json!({
                "BodyExample": {
                    "summary": "Short summary",
                    "description": "Longer description",
                    "value": { "hello": "world" }
                }
            }),
        );

        let media = Content::builder()
            .schema(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                "#/components/schemas/User",
            ))))
            .examples_from_iter([(
                "body",
                RefOr::Ref(Ref::new(
                    "https://example.com/openapi.yaml#/components/examples/BodyExample",
                )),
            )])
            .build();

        let body = RequestBodyBuilder::new()
            .content("application/json", media)
            .build();

        let def =
            extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), Some(&components))
                .expect("Failed to extract request body type")
                .expect("Missing value");
        assert_eq!(
            def.example,
            Some(ExampleValue::data_with_meta(
                json!({"hello": "world"}),
                Some("Short summary".to_string()),
                Some("Longer description".to_string())
            ))
        );
    }

    #[test]
    fn test_extract_request_body_example_ref_overrides_metadata() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            self_uri: Some("https://example.com/openapi.yaml".to_string()),
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "examples".to_string(),
            json!({
                "Greeting": {
                    "summary": "Original summary",
                    "description": "Original description",
                    "value": { "hello": "world" }
                }
            }),
        );

        let raw = json!({
            "content": {
                "application/json": {
                    "schema": { "type": "object" },
                    "examples": {
                        "greeting": {
                            "$ref": "https://example.com/openapi.yaml#/components/examples/Greeting",
                            "summary": "Override summary",
                            "description": "Override description"
                        }
                    }
                }
            }
        });

        let shim: ShimRequestBody =
            serde_json::from_value(raw).expect("Failed to parse from value");
        let def = extract_request_body_type(&RefOr::T(shim), Some(&components))
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(
            def.example,
            Some(ExampleValue::data_with_meta(
                json!({ "hello": "world" }),
                Some("Override summary".to_string()),
                Some("Override description".to_string())
            ))
        );
    }

    #[test]
    fn test_extract_request_body_example_external_value() {
        let media = Content::builder()
            .schema(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                "#/components/schemas/User",
            ))))
            .examples_from_iter([(
                "external",
                utoipa::openapi::example::ExampleBuilder::new()
                    .external_value("https://example.com/example.json"),
            )])
            .build();

        let body = RequestBodyBuilder::new()
            .content("application/json", media)
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(
            def.example,
            Some(ExampleValue::external(json!(
                "https://example.com/example.json"
            )))
        );
    }

    #[test]
    fn test_extract_request_body_example_data_value() {
        let raw = json!({
            "content": {
                "application/json": {
                    "examples": {
                        "payload": {
                            "dataValue": { "id": 42, "name": "Ada" }
                        }
                    }
                }
            }
        });

        let body: ShimRequestBody =
            serde_json::from_value(raw).expect("Failed to parse from value");
        let def = extract_request_body_type(&RefOr::T(body), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(
            def.example,
            Some(ExampleValue::data(json!({ "id": 42, "name": "Ada" })))
        );
    }

    #[test]
    fn test_extract_request_body_example_metadata() {
        let raw = json!({
            "content": {
                "application/json": {
                    "examples": {
                        "payload": {
                            "summary": "Short summary",
                            "description": "Longer description",
                            "dataValue": { "id": 7 }
                        }
                    }
                }
            }
        });

        let body: ShimRequestBody =
            serde_json::from_value(raw).expect("Failed to parse from value");
        let def = extract_request_body_type(&RefOr::T(body), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");

        let example = def.example.expect("example missing");
        assert_eq!(example.summary.as_deref(), Some("Short summary"));
        assert_eq!(example.description.as_deref(), Some("Longer description"));
        assert_eq!(example.value, json!({ "id": 7 }));
    }

    #[test]
    fn test_extract_request_body_example_serialized_value_json() {
        let raw = json!({
            "content": {
                "application/json": {
                    "examples": {
                        "payload": {
                            "serializedValue": "{\"id\": 7, \"name\": \"Grace\"}"
                        }
                    }
                }
            }
        });

        let body: ShimRequestBody =
            serde_json::from_value(raw).expect("Failed to parse from value");
        let def = extract_request_body_type(&RefOr::T(body), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(
            def.example,
            Some(ExampleValue::data(json!({ "id": 7, "name": "Grace" })))
        );
    }

    #[test]
    fn test_extract_request_body_example_serialized_value_text() {
        let raw = json!({
            "content": {
                "text/plain": {
                    "examples": {
                        "payload": {
                            "serializedValue": "hello%20world"
                        }
                    }
                }
            }
        });

        let body: ShimRequestBody =
            serde_json::from_value(raw).expect("Failed to parse from value");
        let def = extract_request_body_type(&RefOr::T(body), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(
            def.example,
            Some(ExampleValue::serialized(json!("hello%20world")))
        );
        assert_eq!(def.format, BodyFormat::Text);
    }

    #[test]
    fn test_extract_request_body_item_schema_sequential_json() {
        let raw = json!({
            "content": {
                "application/jsonl": {
                    "itemSchema": {
                        "type": "string"
                    }
                }
            }
        });

        let body: ShimRequestBody =
            serde_json::from_value(raw).expect("Failed to parse from value");
        let def = extract_request_body_type(&RefOr::T(body), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.ty, "Vec<String>");
        assert_eq!(def.media_type, "application/jsonl");
    }

    #[test]
    fn test_extract_request_body_item_schema_text_event_stream() {
        let raw = json!({
            "content": {
                "text/event-stream": {
                    "itemSchema": {
                        "type": "object",
                        "properties": {
                            "data": { "type": "string" }
                        }
                    }
                }
            }
        });

        let body: ShimRequestBody =
            serde_json::from_value(raw).expect("Failed to parse from value");
        let def = extract_request_body_type(&RefOr::T(body), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.ty, "Vec<serde_json::Value>");
        assert_eq!(def.media_type, "text/event-stream");
        assert_eq!(def.format, BodyFormat::Text);
    }

    #[test]
    fn test_extract_request_body_item_schema_multipart_mixed() {
        let raw = json!({
            "content": {
                "multipart/mixed": {
                    "itemSchema": {
                        "type": "string"
                    }
                }
            }
        });

        let body: ShimRequestBody =
            serde_json::from_value(raw).expect("Failed to parse from value");
        let def = extract_request_body_type(&RefOr::T(body), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.ty, "Vec<String>");
        assert_eq!(def.media_type, "multipart/mixed");
        assert_eq!(def.format, BodyFormat::Multipart);
    }

    #[test]
    fn test_request_body_schema_false_optional_skips() {
        let raw = json!({
            "required": false,
            "content": {
                "application/json": {
                    "schema": false
                }
            }
        });

        let body: ShimRequestBody =
            serde_json::from_value(raw).expect("Failed to parse from value");
        let def = extract_request_body_type(&RefOr::T(body), None)
            .expect("Failed to extract request body type");
        assert!(def.is_none());
    }

    #[test]
    fn test_request_body_schema_false_required_errors() {
        let raw = json!({
            "required": true,
            "content": {
                "application/json": {
                    "schema": false
                }
            }
        });

        let body: ShimRequestBody =
            serde_json::from_value(raw).expect("Failed to parse from value");
        let err = extract_request_body_type(&RefOr::T(body), None).expect_err("expected error");
        assert!(format!("{err}").contains("schema is 'false'"));
    }

    #[test]
    fn test_extract_multipart_with_encoding_and_headers() {
        // Encoding with Content-Type and Custom Header
        let png_encoding = EncodingBuilder::new()
            .content_type(Some("image/png".to_string()))
            .header(
                "X-Image-Id",
                HeaderBuilder::new()
                    .schema(RefOr::Ref(utoipa::openapi::Ref::new(
                        "#/components/schemas/Uuid",
                    )))
                    .build(),
            )
            .build();

        let json_encoding = EncodingBuilder::new()
            .content_type(Some("application/json".to_string()))
            .style(Some(ParameterStyle::Form))
            .explode(Some(false))
            .allow_reserved(Some(true))
            .build();

        // ContentBuilder::encoding takes (name, encoding) one by one
        let media = Content::builder()
            .schema(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                "#/components/schemas/Upload",
            ))))
            .encoding("profileImage", png_encoding)
            .encoding("metadata", json_encoding)
            .build();

        let body = RequestBodyBuilder::new()
            .content("multipart/form-data", media)
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");

        assert_eq!(def.ty, "Upload");
        assert_eq!(def.media_type, "multipart/form-data");
        assert_eq!(def.format, BodyFormat::Multipart);

        let enc = def.encoding.expect("Missing encoding");
        let profile = enc.get("profileImage").expect("Failed to get");
        assert_eq!(profile.content_type.as_deref(), Some("image/png"));
        assert_eq!(
            profile.headers.get("X-Image-Id").map(|s| s.as_str()),
            Some("Uuid")
        );

        let meta = enc.get("metadata").expect("Failed to get");
        assert_eq!(meta.content_type.as_deref(), Some("application/json"));
        assert!(meta.headers.is_empty());
        assert_eq!(
            meta.style,
            Some(crate::openapi::parse::models::ParamStyle::Form)
        );
        assert_eq!(meta.explode, Some(false));
        assert_eq!(meta.allow_reserved, Some(true));
    }

    #[test]
    fn test_extract_encoding_headers_ignores_content_type() {
        let encoding = EncodingBuilder::new()
            .content_type(Some("image/png".to_string()))
            .header(
                "Content-Type",
                HeaderBuilder::new()
                    .schema(RefOr::Ref(utoipa::openapi::Ref::new(
                        "#/components/schemas/String",
                    )))
                    .build(),
            )
            .header(
                "X-Trace-Id",
                HeaderBuilder::new()
                    .schema(RefOr::Ref(utoipa::openapi::Ref::new(
                        "#/components/schemas/Uuid",
                    )))
                    .build(),
            )
            .build();

        let media = Content::builder()
            .schema(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                "#/components/schemas/Upload",
            ))))
            .encoding("file", encoding)
            .build();

        let body = RequestBodyBuilder::new()
            .content("multipart/form-data", media)
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        let enc = def.encoding.expect("Missing encoding");
        let file = enc.get("file").expect("Failed to get");
        assert!(!file.headers.contains_key("Content-Type"));
        assert_eq!(
            file.headers.get("X-Trace-Id").map(|s| s.as_str()),
            Some("Uuid")
        );
    }

    #[test]
    fn test_extract_encoding_headers_from_ref() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Encoding Test, version: 1.0}
paths:
  /upload:
    post:
      requestBody:
        content:
          multipart/form-data:
            schema:
              type: object
              properties:
                file:
                  type: string
            encoding:
              file:
                headers:
                  X-Rate-Limit:
                    $ref: '#/components/headers/RateLimit'
      responses:
        '200': { description: ok }
components:
  headers:
    RateLimit:
      schema:
        type: integer
        format: int32
"#;

        let routes = parse_openapi_routes(yaml).expect("Failed to parse openapi routes");
        let body = routes[0]
            .request_body
            .as_ref()
            .expect("Failed to get reference");
        let enc = body.encoding.as_ref().expect("Failed to get reference");
        let header_ty = enc
            .get("file")
            .and_then(|info| info.headers.get("X-Rate-Limit"))
            .expect("Failed to and then");
        assert_eq!(header_ty, "i32");
    }

    #[test]
    fn test_extract_multipart_prefix_and_item_encoding() {
        let raw = json!({
            "content": {
                "multipart/mixed": {
                    "itemSchema": { "type": "string" },
                    "prefixEncoding": [
                        { "contentType": "application/json" },
                        { "contentType": "image/png" }
                    ],
                    "itemEncoding": { "contentType": "text/plain" }
                }
            }
        });

        let body: ShimRequestBody =
            serde_json::from_value(raw).expect("Failed to parse from value");
        let def = extract_request_body_type(&RefOr::T(body), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");

        let prefix = def.prefix_encoding.expect("expected prefixEncoding");
        assert_eq!(prefix.len(), 2);
        assert_eq!(prefix[0].content_type.as_deref(), Some("application/json"));
        assert_eq!(prefix[1].content_type.as_deref(), Some("image/png"));
        assert_eq!(
            def.item_encoding
                .as_ref()
                .and_then(|e| e.content_type.as_deref()),
            Some("text/plain")
        );
    }

    #[test]
    fn test_extract_nested_encoding_object() {
        let raw = json!({
            "content": {
                "multipart/form-data": {
                    "schema": {
                        "type": "object",
                        "properties": {
                            "payload": { "type": "object" }
                        }
                    },
                    "encoding": {
                        "payload": {
                            "contentType": "multipart/mixed",
                            "encoding": {
                                "part": { "contentType": "application/json" }
                            },
                            "prefixEncoding": [
                                { "contentType": "text/plain" }
                            ],
                            "itemEncoding": { "contentType": "application/octet-stream" }
                        }
                    }
                }
            }
        });

        let body: ShimRequestBody =
            serde_json::from_value(raw).expect("Failed to parse from value");
        let def = extract_request_body_type(&RefOr::T(body), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        let payload = def
            .encoding
            .as_ref()
            .and_then(|enc| enc.get("payload"))
            .expect("payload encoding");

        assert_eq!(payload.content_type.as_deref(), Some("multipart/mixed"));
        let nested = payload.encoding.as_ref().expect("nested encoding");
        assert_eq!(
            nested
                .get("part")
                .and_then(|info| info.content_type.as_deref()),
            Some("application/json")
        );
        assert_eq!(
            payload
                .prefix_encoding
                .as_ref()
                .and_then(|items| items.first())
                .and_then(|info| info.content_type.as_deref()),
            Some("text/plain")
        );
        assert_eq!(
            payload
                .item_encoding
                .as_ref()
                .and_then(|info| info.content_type.as_deref()),
            Some("application/octet-stream")
        );
    }

    #[test]
    fn test_extract_multipart_encoding_conflict_rejected() {
        let raw = json!({
            "content": {
                "multipart/mixed": {
                    "schema": { "type": "object" },
                    "encoding": {
                        "file": { "contentType": "application/octet-stream" }
                    },
                    "itemEncoding": { "contentType": "text/plain" }
                }
            }
        });

        let body: ShimRequestBody =
            serde_json::from_value(raw).expect("Failed to parse from value");
        let err = extract_request_body_type(&RefOr::T(body), None).expect_err("expected error");
        assert!(format!("{err}").contains("encoding"));
    }

    #[test]
    fn test_extract_form_no_encoding() {
        let request_body = RequestBodyBuilder::new()
            .content(
                "application/x-www-form-urlencoded",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/Login",
                )))),
            )
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(request_body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.ty, "Login");
        assert_eq!(def.media_type, "application/x-www-form-urlencoded");
        assert_eq!(def.format, BodyFormat::Form);
        assert!(def.encoding.is_none());
    }

    #[test]
    fn test_extract_vendor_json_body() {
        let body = RequestBodyBuilder::new()
            .content(
                "application/vnd.api+json",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/User",
                )))),
            )
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.ty, "User");
        assert_eq!(def.media_type, "application/vnd.api+json");
        assert_eq!(def.format, BodyFormat::Json);
    }

    #[test]
    fn test_extract_ndjson_body_as_json() {
        let body = RequestBodyBuilder::new()
            .content(
                "application/x-ndjson",
                Content::new::<RefOr<utoipa::openapi::Schema>>(None),
            )
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.ty, "Vec<serde_json::Value>");
        assert_eq!(def.media_type, "application/x-ndjson");
        assert_eq!(def.format, BodyFormat::Json);
    }

    #[test]
    fn test_extract_text_body_without_schema() {
        let body = RequestBodyBuilder::new()
            .content(
                "text/plain",
                Content::new::<RefOr<utoipa::openapi::Schema>>(None),
            )
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.ty, "String");
        assert_eq!(def.media_type, "text/plain");
        assert_eq!(def.format, BodyFormat::Text);
        assert!(def.encoding.is_none());
    }

    #[test]
    fn test_extract_xml_body_as_text() {
        let body = RequestBodyBuilder::new()
            .content(
                "application/xml",
                Content::new::<RefOr<utoipa::openapi::Schema>>(None),
            )
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.ty, "String");
        assert_eq!(def.media_type, "application/xml");
        assert_eq!(def.format, BodyFormat::Text);
    }

    #[test]
    fn test_extract_binary_body_without_schema() {
        let body = RequestBodyBuilder::new()
            .content(
                "application/octet-stream",
                Content::new::<RefOr<utoipa::openapi::Schema>>(None),
            )
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.ty, "Vec<u8>");
        assert_eq!(def.media_type, "application/octet-stream");
        assert_eq!(def.format, BodyFormat::Binary);
        assert!(def.encoding.is_none());
    }

    #[test]
    fn test_extract_request_body_ref_with_self() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            self_uri: Some("https://example.com/openapi.yaml".to_string()),
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "requestBodies".to_string(),
            json!({
                "CreateThing": {
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/Thing" }
                        }
                    }
                }
            }),
        );

        let body_ref = RefOr::Ref(utoipa::openapi::Ref::new(
            "https://example.com/openapi.yaml#/components/requestBodies/CreateThing",
        ));
        let def = extract_request_body_type(&body_ref, Some(&components))
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.ty, "Thing");
        assert_eq!(def.media_type, "application/json");
        assert_eq!(def.format, BodyFormat::Json);
        assert!(!def.required);
    }

    #[test]
    fn test_extract_request_body_ref_description_override() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            self_uri: Some("https://example.com/openapi.yaml".to_string()),
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "requestBodies".to_string(),
            json!({
                "CreateThing": {
                    "description": "original",
                    "content": {
                        "application/json": {
                            "schema": { "type": "string" }
                        }
                    }
                }
            }),
        );

        let mut ref_body = utoipa::openapi::Ref::new(
            "https://example.com/openapi.yaml#/components/requestBodies/CreateThing",
        );
        ref_body.description = "override".to_string();

        let body_ref = RefOr::Ref(ref_body);
        let def = extract_request_body_type(&body_ref, Some(&components))
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.description.as_deref(), Some("override"));
    }

    #[test]
    fn test_extract_request_body_media_type_ref() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            self_uri: None,
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "mediaTypes".to_string(),
            json!({
                "JsonBody": {
                    "schema": { "type": "string" }
                }
            }),
        );

        let body_json = json!({
            "content": {
                "application/json": {
                    "$ref": "#/components/mediaTypes/JsonBody"
                }
            }
        });
        let shim: ShimRequestBody =
            serde_json::from_value(body_json).expect("Failed to parse from value");
        let def = extract_request_body_type(&RefOr::T(shim), Some(&components))
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.ty, "String");
        assert_eq!(def.media_type, "application/json");
        assert_eq!(def.format, BodyFormat::Json);
    }

    #[test]
    fn test_extract_request_body_application_wildcard() {
        let body_json = json!({
            "content": {
                "application/*": {}
            }
        });
        let shim: ShimRequestBody =
            serde_json::from_value(body_json).expect("Failed to parse from value");
        let def = extract_request_body_type(&RefOr::T(shim), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(def.media_type, "application/*");
        assert_eq!(def.format, BodyFormat::Binary);
        assert_eq!(def.ty, "Vec<u8>");
    }

    #[test]
    fn test_extract_request_body_required_true() {
        let body = RequestBodyBuilder::new()
            .required(Some(utoipa::openapi::Required::True))
            .content(
                "application/json",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/User",
                )))),
            )
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert!(def.required);
    }

    #[test]
    #[allow(deprecated)]
    fn test_extract_request_body_schema_example_fallback() {
        let schema = Schema::Object(
            utoipa::openapi::schema::ObjectBuilder::new()
                .schema_type(utoipa::openapi::schema::Type::Object)
                .example(Some(json!({"name": "Ada"})))
                .build(),
        );
        let media = Content::builder().schema(Some(RefOr::T(schema))).build();

        let body = RequestBodyBuilder::new()
            .content("application/json", media)
            .build();

        let def = extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), None)
            .expect("Failed to extract request body type")
            .expect("Missing value");
        assert_eq!(
            def.example,
            Some(ExampleValue::data(json!({"name": "Ada"})))
        );
    }

    #[test]
    fn test_extract_request_body_schema_example_ref() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            self_uri: Some("https://example.com/openapi.yaml".to_string()),
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "schemas".to_string(),
            json!({
                "Upload": {
                    "example": { "filename": "demo.png" }
                }
            }),
        );

        let media = Content::builder()
            .schema(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                "https://example.com/openapi.yaml#/components/schemas/Upload",
            ))))
            .build();

        let body = RequestBodyBuilder::new()
            .content("application/json", media)
            .build();

        let def =
            extract_request_body_type(&RefOr::T(ShimRequestBody::from(body)), Some(&components))
                .expect("Failed to extract request body type")
                .expect("Missing value");
        assert_eq!(
            def.example,
            Some(ExampleValue::data(json!({"filename": "demo.png"})))
        );

        // Comprehensive tests for remaining branches in body.rs:
        // 1. Unresolved Ref & empty content
        let missing_ref = RefOr::Ref(Ref::new("#/components/requestBodies/Missing"));
        assert!(extract_request_body_type(&missing_ref, Some(&components))
            .expect("ok")
            .is_none());
        assert!(extract_request_body_raw(&missing_ref, None).is_none());
        let inline_shim: ShimRequestBody = serde_json::from_value(json!({
            "content": {}
        }))
        .expect("empty content");
        assert!(extract_request_body_raw(&RefOr::T(inline_shim.clone()), None).is_some());
        assert!(extract_request_body_raw(
            &RefOr::Ref(Ref::new(
                "https://example.com/openapi.yaml#/components/requestBodies/Missing"
            )),
            Some(&components)
        )
        .is_none());
        assert!(extract_request_body_type(&RefOr::T(inline_shim), None)
            .expect("ok")
            .is_none());

        // 2. itemSchema: false (required vs optional) & non-sequential itemSchema
        let item_false_opt: ShimRequestBody = serde_json::from_value(json!({
            "required": false,
            "content": { "application/json": { "itemSchema": false } }
        }))
        .expect("item false opt");
        assert!(extract_request_body_type(&RefOr::T(item_false_opt), None)
            .expect("ok")
            .is_none());
        let item_false_req: ShimRequestBody = serde_json::from_value(json!({
            "required": true,
            "content": { "application/json": { "itemSchema": false } }
        }))
        .expect("item false req");
        assert!(extract_request_body_type(&RefOr::T(item_false_req), None).is_err());

        let item_non_seq: ShimRequestBody = serde_json::from_value(json!({
            "content": { "application/json": { "itemSchema": { "type": "integer" } } }
        }))
        .expect("item non seq");
        let def_non_seq = extract_request_body_type(&RefOr::T(item_non_seq), None)
            .expect("ok")
            .expect("some");
        assert_eq!(def_non_seq.ty, "i32");

        // 3. Fallback media types (*/*, custom/unknown, image/png, audio/mp3, video/mp4, application/pdf)
        for mt in [
            "*/*",
            "custom/unknown",
            "image/png",
            "audio/mp3",
            "video/mp4",
            "application/pdf",
        ] {
            let s: ShimRequestBody = serde_json::from_value(json!({
                "content": { mt: {} }
            }))
            .expect("mt");
            let d = extract_request_body_type(&RefOr::T(s), None)
                .expect("ok")
                .expect("some");
            assert_eq!(d.ty, "Vec<u8>");
        }
        assert_eq!(infer_binary_type("text/custom"), "String");
        assert_eq!(
            default_body_type(BodyFormat::Form, "application/x-www-form-urlencoded"),
            "serde_json::Value"
        );
        assert_eq!(
            default_body_type(BodyFormat::Multipart, "multipart/form-data"),
            "Multipart"
        );

        // 4. Registry requestBody & mediaTypes resolution
        let mut reg = DocumentRegistry::new();
        reg.register_openapi_yaml(
            "https://example.com/ext.yaml",
            r#"
openapi: 3.2.0
info: { title: Ext, version: "1.0" }
paths: {}
components:
  requestBodies:
    ExtBody:
      required: true
      content:
        application/json:
          $ref: '#/components/mediaTypes/ExtMedia'
  mediaTypes:
    ExtMedia:
      schema:
        type: string
  headers:
    ExtHdr:
      schema:
        type: string
"#,
        )
        .expect("reg");
        let ext_ref = RefOr::Ref(Ref::new(
            "https://example.com/ext.yaml#/components/requestBodies/ExtBody",
        ));
        let ext_media_body: ShimRequestBody = serde_json::from_value(json!({
            "content": {
                "application/json": {
                    "$ref": "https://example.com/ext.yaml#/components/mediaTypes/ExtMedia"
                }
            }
        }))
        .expect("ext_media_body");
        let ext_media_def = extract_request_body_type_with_registry(
            &RefOr::T(ext_media_body),
            None,
            Some(&reg),
            None,
        )
        .expect("ok")
        .expect("some");
        assert_eq!(ext_media_def.ty, "String");

        let only_ct_enc = json!({
            "headers": {
                "Content-Type": { "schema": { "type": "string" } }
            }
        });
        let parsed_only_ct = parse_encoding_value(&only_ct_enc, None, None, None).expect("only ct");
        assert!(parsed_only_ct.headers.is_empty());

        let _ = extract_positional_encoding(None, None, None, None).expect("none");
        assert!(extract_request_body_raw_with_registry(&ext_ref, None, Some(&reg), None).is_some());

        // Media type ref cycle & non-object media in raw
        let mut cycle_comps = ShimComponents::default();
        cycle_comps.extra.insert(
            "mediaTypes".to_string(),
            json!({
                "CycleM": { "$ref": "#/components/mediaTypes/CycleM" }
            }),
        );
        let cycle_body: ShimRequestBody = serde_json::from_value(json!({
            "content": {
                "application/json": { "$ref": "#/components/mediaTypes/CycleM" }
            }
        }))
        .expect("cycle_body");
        let _ = extract_request_body_type(&RefOr::T(cycle_body), Some(&cycle_comps)).expect("ok");
        assert!(raw_media_for_type(
            &json!({ "content": { "application/json": { "$ref": "#/components/mediaTypes/MissingM" } } }),
            "application/json",
            Some(&cycle_comps),
            Some(&reg),
            None
        )
        .is_some());
        assert!(raw_media_for_type(
            &json!({ "content": { "application/json": "not_an_obj" } }),
            "application/json",
            None,
            None,
            None
        )
        .is_none());

        // 5. Examples: rawexample, cycle in examples ref, externalValue in raw, non-object/null in examples,
        // utoipa Example with summary/description/none, Ref with summary/description override, Schema examples array (Object, Array, OneOf, AnyOf, AllOf, other)
        let mut ex_comps = ShimComponents::default();
        ex_comps.extra.insert(
            "examples".to_string(),
            json!({
                "CycleEx": { "$ref": "#/components/examples/CycleEx" },
                "ExtEx": {
                    "summary": "S",
                    "description": "D",
                    "externalValue": "https://example.com/ex.json"
                },
                "ValEx": {
                    "value": { "k": "v" }
                },
                "EmptyEx": {}
            }),
        );
        ex_comps.extra.insert(
            "schemas".to_string(),
            json!({
                "WithExamplesArr": {
                    "examples": ["first_ex"]
                },
                "NotObjSchema": "bad"
            }),
        );
        let raw_ex_body: ShimRequestBody = serde_json::from_value(json!({
            "content": {
                "application/json": {
                    "example": "top_ex"
                }
            }
        }))
        .expect("raw_ex_body");
        let d_ex = extract_request_body_type(&RefOr::T(raw_ex_body), None)
            .expect("ok")
            .expect("some");
        assert_eq!(d_ex.example, Some(ExampleValue::data(json!("top_ex"))));

        let mut vis = HashSet::new();
        assert!(extract_example_value(&json!(null), None, "application/json", &mut vis).is_none());
        assert_eq!(
            extract_example_value(&json!("primitive_val"), None, "application/json", &mut vis),
            Some(ExampleValue::data(json!("primitive_val")))
        );
        assert_eq!(
            extract_example_value(
                &json!({ "serializedValue": 123 }),
                None,
                "application/json",
                &mut vis
            ),
            Some(ExampleValue::serialized(json!(123)))
        );

        let raw_examples_misc: ShimRequestBody = serde_json::from_value(json!({
            "content": {
                "application/json": {
                    "examples": {
                        "a_cycle": { "$ref": "#/components/examples/CycleEx" },
                        "b_empty_obj": {},
                        "d_invalid_json_ser": { "serializedValue": "{bad json" }
                    }
                }
            }
        }))
        .expect("misc");
        let d_misc = extract_request_body_type(&RefOr::T(raw_examples_misc), Some(&ex_comps))
            .expect("ok")
            .expect("some");
        assert_eq!(
            d_misc.example,
            Some(ExampleValue::serialized(json!("{bad json")))
        );

        let raw_ext_val: ShimRequestBody = serde_json::from_value(json!({
            "content": {
                "application/json": {
                    "examples": {
                        "ext": { "externalValue": "https://example.com/a" }
                    }
                }
            }
        }))
        .expect("ext");
        let d_ext_val = extract_request_body_type(&RefOr::T(raw_ext_val), None)
            .expect("ok")
            .expect("some");
        assert_eq!(
            d_ext_val.example,
            Some(ExampleValue::external(json!("https://example.com/a")))
        );

        // Typed media examples (when raw_body has no examples)
        let mut r_override = Ref::new("#/components/examples/ExtEx");
        r_override.summary = "SumOvr".to_string();
        r_override.description = "DescOvr".to_string();
        let typed_media = Content::builder()
            .examples_from_iter([
                (
                    "empty",
                    RefOr::T(utoipa::openapi::example::ExampleBuilder::new().build()),
                ),
                (
                    "ext_inline",
                    RefOr::T(
                        utoipa::openapi::example::ExampleBuilder::new()
                            .external_value("https://example.com/inline.json")
                            .build(),
                    ),
                ),
            ])
            .build();
        let _ = extract_media_example(
            &typed_media,
            Some(&ex_comps),
            None,
            None,
            None,
            "application/json",
        );
        let typed_media_ref_ovr = Content::builder()
            .examples_from_iter([("ref_ext", RefOr::Ref(r_override))])
            .build();
        let _ = extract_media_example(
            &typed_media_ref_ovr,
            Some(&ex_comps),
            None,
            None,
            None,
            "application/json",
        );
        let typed_media_val = Content::builder()
            .examples_from_iter([(
                "with_meta",
                RefOr::T(
                    utoipa::openapi::example::ExampleBuilder::new()
                        .summary("S")
                        .description("D")
                        .value(Some(json!(1)))
                        .build(),
                ),
            )])
            .build();
        let _ = extract_media_example(
            &typed_media_val,
            Some(&ex_comps),
            None,
            None,
            None,
            "application/json",
        );
        let typed_media_ref_val = Content::builder()
            .examples_from_iter([
                (
                    "empty_ref",
                    RefOr::Ref(Ref::new("#/components/examples/EmptyEx")),
                ),
                (
                    "val_ref",
                    RefOr::Ref(Ref::new("#/components/examples/ValEx")),
                ),
            ])
            .build();
        let _ = extract_media_example(
            &typed_media_ref_val,
            Some(&ex_comps),
            None,
            None,
            None,
            "application/json",
        );
        let typed_media_ref_only = Content::builder()
            .examples_from_iter([(
                "ext_ref",
                RefOr::Ref(Ref::new("#/components/examples/ExtEx")),
            )])
            .build();
        let _ = extract_media_example(
            &typed_media_ref_only,
            Some(&ex_comps),
            None,
            None,
            None,
            "application/json",
        );
        #[allow(deprecated)]
        let typed_media_top_ex = Content::builder().example(Some(json!("typed_top"))).build();
        assert_eq!(
            extract_media_example(
                &typed_media_top_ex,
                None,
                None,
                None,
                None,
                "application/json"
            ),
            Some(ExampleValue::data(json!("typed_top")))
        );

        // Schema example variants
        for s in [
            Schema::Object(
                utoipa::openapi::schema::ObjectBuilder::new()
                    .examples([json!("obj_ex")])
                    .build(),
            ),
            Schema::Array(
                utoipa::openapi::schema::ArrayBuilder::new()
                    .examples([json!("arr_ex")])
                    .build(),
            ),
            Schema::OneOf(
                utoipa::openapi::schema::OneOfBuilder::new()
                    .examples([json!("oneof_ex")])
                    .build(),
            ),
            Schema::AnyOf(
                utoipa::openapi::schema::AnyOfBuilder::new()
                    .examples([json!("anyof_ex")])
                    .build(),
            ),
            Schema::AllOf(
                utoipa::openapi::schema::AllOfBuilder::new()
                    .examples([json!("allof_ex")])
                    .build(),
            ),
        ] {
            let m = Content::builder().schema(Some(RefOr::T(s))).build();
            let _ = extract_media_example(&m, None, None, None, None, "application/json");
        }
        let m_schema_ref = Content::builder()
            .schema(Some(RefOr::Ref(Ref::new(
                "#/components/schemas/WithExamplesArr",
            ))))
            .build();
        assert_eq!(
            extract_media_example(
                &m_schema_ref,
                Some(&ex_comps),
                None,
                None,
                None,
                "application/json"
            ),
            Some(ExampleValue::data(json!("first_ex")))
        );
        let m_schema_bad = Content::builder()
            .schema(Some(RefOr::Ref(Ref::new(
                "#/components/schemas/NotObjSchema",
            ))))
            .build();
        assert!(extract_media_example(
            &m_schema_bad,
            Some(&ex_comps),
            None,
            None,
            None,
            "application/json"
        )
        .is_none());

        // 6. Positional encoding & encoding headers edge cases
        let conflict_prefix: ShimRequestBody = serde_json::from_value(json!({
            "content": {
                "multipart/mixed": {
                    "encoding": { "a": { "contentType": "text/plain" } },
                    "prefixEncoding": [{ "contentType": "text/plain" }]
                }
            }
        }))
        .expect("conflict_prefix");
        assert!(extract_request_body_type(&RefOr::T(conflict_prefix), None).is_err());

        let bad_prefix: ShimRequestBody = serde_json::from_value(json!({
            "content": {
                "multipart/mixed": {
                    "prefixEncoding": "not_an_array"
                }
            }
        }))
        .expect("bad_prefix");
        assert!(extract_request_body_type(&RefOr::T(bad_prefix), None).is_err());

        let empty_prefix: ShimRequestBody = serde_json::from_value(json!({
            "content": {
                "multipart/mixed": {
                    "prefixEncoding": [],
                    "encoding": {}
                }
            }
        }))
        .expect("empty_prefix");
        let _ = extract_request_body_type(&RefOr::T(empty_prefix), None).expect("ok");

        assert!(parse_encoding_value(&json!("not_an_encoding"), None, None, None).is_err());

        assert_eq!(
            resolve_encoding_header_type(&json!("not_an_obj"), None, None, None).expect("ok"),
            "String"
        );
        let enc_rich = json!({
            "headers": {
                "Content-Type": { "schema": { "type": "string" } },
                "X-Bool-True": { "schema": true },
                "X-No-Schema-Or-Content": {},
                "X-Content-Schema": {
                    "content": {
                        "application/json": { "schema": { "type": "integer", "format": "int32" } }
                    }
                },
                "X-Content-Item-Seq": {
                    "content": {
                        "application/jsonl": { "itemSchema": { "type": "string" } }
                    }
                },
                "X-Content-Item-NonSeq": {
                    "content": {
                        "application/json": { "itemSchema": { "type": "string" } }
                    }
                },
                "X-Content-Empty": {
                    "content": {
                        "text/plain": {}
                    }
                },
                "X-Ext-Hdr": {
                    "$ref": "https://example.com/ext.yaml#/components/headers/ExtHdr"
                }
            },
            "encoding": {},
            "prefixEncoding": []
        });
        let parsed_enc =
            parse_encoding_value(&enc_rich, None, Some(&reg), None).expect("parsed_enc");
        assert_eq!(
            parsed_enc.headers.get("X-Bool-True").map(String::as_str),
            Some("String")
        );
        assert_eq!(
            parsed_enc
                .headers
                .get("X-Content-Schema")
                .map(String::as_str),
            Some("i32")
        );
        assert_eq!(
            parsed_enc
                .headers
                .get("X-Content-Item-Seq")
                .map(String::as_str),
            Some("Vec<String>")
        );
        assert_eq!(
            parsed_enc
                .headers
                .get("X-Content-Item-NonSeq")
                .map(String::as_str),
            Some("String")
        );
        assert_eq!(
            parsed_enc
                .headers
                .get("X-Content-Empty")
                .map(String::as_str),
            Some("String")
        );
        assert_eq!(
            parsed_enc.headers.get("X-Ext-Hdr").map(String::as_str),
            Some("String")
        );

        let mut hdr_cycle_comps = ShimComponents::default();
        hdr_cycle_comps.extra.insert(
            "headers".to_string(),
            json!({
                "CycleH": { "$ref": "#/components/headers/CycleH" },
                "LocalH": { "schema": { "type": "string" } }
            }),
        );
        assert_eq!(
            resolve_encoding_header_type(
                &json!({ "$ref": "#/components/headers/LocalH" }),
                Some(&hdr_cycle_comps),
                None,
                None
            )
            .expect("local hdr"),
            "String"
        );
        let mut typed_enc_map = BTreeMap::new();
        typed_enc_map.insert(
            "part".to_string(),
            EncodingBuilder::new()
                .header(
                    "Content-Type",
                    HeaderBuilder::new()
                        .schema(RefOr::Ref(Ref::new("#/components/schemas/String")))
                        .build(),
                )
                .header(
                    "X-Custom",
                    HeaderBuilder::new()
                        .schema(RefOr::Ref(Ref::new("#/components/schemas/Uuid")))
                        .build(),
                )
                .build(),
        );
        let _ =
            extract_encoding_map_with_raw(&typed_enc_map, None, None, None, None).expect("typed");
        let _ =
            extract_encoding_map_with_raw(&BTreeMap::new(), None, None, None, None).expect("empty");
        for bad_hdr in [
            json!({ "schema": false }),
            json!({ "schema": 123 }),
            json!({ "content": "not_an_obj" }),
            json!({ "content": {} }),
            json!({ "content": { "application/json": "not_an_obj" } }),
            json!({ "content": { "application/json": { "schema": 123 } } }),
            json!({ "content": { "application/json": { "itemSchema": 123 } } }),
            json!({ "$ref": "#/components/headers/CycleH" }),
            json!({ "$ref": "#/components/headers/MissingH" }),
        ] {
            assert!(
                resolve_encoding_header_type(&bad_hdr, Some(&hdr_cycle_comps), None, None).is_err()
            );
        }

        for style in [
            Some(ParameterStyle::Matrix),
            Some(ParameterStyle::Label),
            Some(ParameterStyle::Form),
            Some(ParameterStyle::Simple),
            Some(ParameterStyle::SpaceDelimited),
            Some(ParameterStyle::PipeDelimited),
            Some(ParameterStyle::DeepObject),
            None,
        ] {
            let _ = map_param_style(style.as_ref());
        }
    }
}
