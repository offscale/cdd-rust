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
use crate::oas::models::{BodyFormat, EncodingInfo, ParamStyle, RequestBodyDefinition};
use crate::oas::ref_utils::extract_component_name;
use crate::oas::resolver::types::map_schema_to_rust_type;
use crate::oas::routes::shims::{ShimComponents, ShimRequestBody};
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use utoipa::openapi::encoding::Encoding;
use utoipa::openapi::example::Example;
use utoipa::openapi::path::ParameterStyle;
use utoipa::openapi::{schema::Schema, RefOr, Required};

/// Extracts the request body type and format from the OpenAPI definition.
///
/// Resolves `$ref` values against `components.requestBodies` when available, including
/// OAS 3.2 `$self`-qualified absolute references.
pub fn extract_request_body_type(
    body: &RefOr<ShimRequestBody>,
    components: Option<&ShimComponents>,
) -> AppResult<Option<RequestBodyDefinition>> {
    let owned_body;
    let (content, required, description, raw_body) = match body {
        RefOr::T(b) => (
            &b.inner.content,
            is_body_required(b),
            b.inner.description.clone(),
            Some(&b.raw),
        ),
        RefOr::Ref(r) => {
            if let Some(mut resolved) = resolve_request_body_from_components(r, components) {
                if !r.description.is_empty() {
                    resolved.inner.description = Some(r.description.clone());
                }
                let required = is_body_required(&resolved);
                let description = resolved.inner.description.clone();
                owned_body = resolved;
                (
                    &owned_body.inner.content,
                    required,
                    description,
                    Some(&owned_body.raw),
                )
            } else {
                return Ok(None);
            }
        }
    };

    let Some((format, media_type, media)) = select_request_media(content) else {
        return Ok(None);
    };

    let raw_media = raw_body.and_then(|raw| raw_media_for_type(raw, media_type, components));
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
    let raw_schema = raw_media.as_ref().and_then(extract_media_schema);
    let item_schema = raw_media.as_ref().and_then(extract_item_schema);

    let ty = if let Some(schema_ref) = &media.schema {
        map_schema_to_rust_type(schema_ref, true)?
    } else if let Some(schema_ref) = raw_schema.as_ref() {
        map_schema_to_rust_type(schema_ref, true)?
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
        BodyFormat::Form | BodyFormat::Multipart => extract_encoding_map(&media.encoding)?,
        _ => None,
    };
    let (prefix_encoding, item_encoding) = match format {
        BodyFormat::Multipart => extract_positional_encoding(raw_media.as_ref())?,
        _ => (None, None),
    };
    if encoding.is_some() && (prefix_encoding.is_some() || item_encoding.is_some()) {
        return Err(AppError::General(
            "MediaType cannot define both 'encoding' and positional 'prefixEncoding'/'itemEncoding'"
                .to_string(),
        ));
    }
    let example = extract_media_example(media, components, raw_media.as_ref(), media_type);

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

fn is_body_required(body: &ShimRequestBody) -> bool {
    matches!(body.inner.required, Some(Required::True))
}

fn select_request_media<'a>(
    content: &'a BTreeMap<String, utoipa::openapi::Content>,
) -> Option<(BodyFormat, &'a str, &'a utoipa::openapi::Content)> {
    // 1. JSON (application/json or +json)
    if let Some(media) = content.get("application/json") {
        return Some((BodyFormat::Json, "application/json", media));
    }

    if let Some((key, media)) = content.iter().find(|(k, _)| is_json_media_type(k)) {
        return Some((BodyFormat::Json, key.as_str(), media));
    }

    // 2. Form URL Encoded
    if let Some(media) = content
        .iter()
        .find(|(k, _)| is_form_media_type(k))
        .map(|(k, v)| (k.as_str(), v))
    {
        return Some((BodyFormat::Form, media.0, media.1));
    }

    // 3. Multipart
    if let Some((key, media)) = content.iter().find(|(k, _)| is_multipart_media_type(k)) {
        return Some((BodyFormat::Multipart, key.as_str(), media));
    }

    // 4. Text
    if let Some((key, media)) = content.iter().find(|(k, _)| is_text_media_type(k)) {
        return Some((BodyFormat::Text, key.as_str(), media));
    }

    // 5. Binary
    if let Some((key, media)) = content.iter().find(|(k, _)| is_binary_media_type(k)) {
        return Some((BodyFormat::Binary, key.as_str(), media));
    }

    // 6. application/* wildcard
    if let Some(media) = content.get("application/*") {
        return Some((BodyFormat::Binary, "application/*", media));
    }

    // 7. */* wildcard
    if let Some(media) = content.get("*/*") {
        return Some((BodyFormat::Binary, "*/*", media));
    }

    // 8. Fallback: take the first media type as binary.
    content
        .iter()
        .next()
        .map(|(k, v)| (BodyFormat::Binary, k.as_str(), v))
}

fn is_json_media_type(media_type: &str) -> bool {
    let normalized = normalize_media_type(media_type);
    normalized == "application/json"
        || normalized.ends_with("+json")
        || normalized == "application/*+json"
        || is_sequential_json_media_type(&normalized)
}

fn is_form_media_type(media_type: &str) -> bool {
    normalize_media_type(media_type) == "application/x-www-form-urlencoded"
}

fn is_multipart_media_type(media_type: &str) -> bool {
    normalize_media_type(media_type).starts_with("multipart/")
}

fn is_text_media_type(media_type: &str) -> bool {
    let normalized = normalize_media_type(media_type);
    normalized.starts_with("text/") || is_xml_media_type(&normalized)
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
    media_type
        .split(';')
        .next()
        .unwrap_or(media_type)
        .trim()
        .to_ascii_lowercase()
}

fn is_sequential_json_media_type(media_type: &str) -> bool {
    let normalized = normalize_media_type(media_type);
    matches!(
        normalized.as_str(),
        "application/jsonl"
            | "application/x-ndjson"
            | "application/json-seq"
            | "application/geo+json-seq"
    ) || normalized.ends_with("+jsonl")
        || normalized.ends_with("+ndjson")
        || normalized.ends_with("+json-seq")
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

fn is_xml_media_type(media_type: &str) -> bool {
    let normalized = normalize_media_type(media_type);
    normalized == "application/xml" || normalized == "text/xml" || normalized.ends_with("+xml")
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
) -> Option<ShimRequestBody> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let ref_name = extract_component_name(&r.ref_location, self_uri, "requestBodies")?;
    if let Some(body_json) = comps
        .extra
        .get("requestBodies")
        .and_then(|m| m.get(&ref_name))
    {
        if let Ok(body) = serde_json::from_value::<ShimRequestBody>(body_json.clone()) {
            return Some(body);
        }
    }
    None
}

fn extract_media_example(
    media: &utoipa::openapi::Content,
    components: Option<&ShimComponents>,
    raw_media: Option<&serde_json::Value>,
    media_type: &str,
) -> Option<serde_json::Value> {
    if let Some(raw) = raw_media {
        if let Some(example) = raw.get("example") {
            return Some(example.clone());
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
        return Some(example.clone());
    }

    for example_ref in media.examples.values() {
        if let Some(value) = extract_example_from_ref_or(example_ref, components) {
            return Some(value);
        }
    }

    if let Some(schema_ref) = &media.schema {
        if let Some(example) = extract_schema_example(schema_ref, components) {
            return Some(example);
        }
    }

    None
}

fn extract_example_from_ref_or(
    example_ref: &RefOr<Example>,
    components: Option<&ShimComponents>,
) -> Option<serde_json::Value> {
    match example_ref {
        RefOr::T(example) => example.value.clone().or_else(|| {
            (!example.external_value.is_empty()).then(|| json!(example.external_value.clone()))
        }),
        RefOr::Ref(r) => resolve_example_from_components(r, components),
    }
}

fn extract_example_value(
    value: &serde_json::Value,
    components: Option<&ShimComponents>,
    media_type: &str,
    visiting: &mut std::collections::HashSet<String>,
) -> Option<serde_json::Value> {
    if let Some(obj) = value.as_object() {
        if let Some(ref_str) = obj.get("$ref").and_then(|v| v.as_str()) {
            return resolve_example_ref(ref_str, components, media_type, visiting);
        }
        if let Some(val) = obj.get("dataValue") {
            return Some(val.clone());
        }
        if let Some(val) = obj.get("value") {
            return Some(val.clone());
        }
        if let Some(val) = obj.get("serializedValue") {
            if let Some(serialized) = val.as_str() {
                if is_json_media_type(media_type) {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(serialized) {
                        return Some(parsed);
                    }
                }
            }
            return Some(val.clone());
        }
        if let Some(val) = obj.get("externalValue") {
            return Some(val.clone());
        }
        return None;
    }

    if !value.is_null() {
        return Some(value.clone());
    }

    None
}

fn resolve_example_ref(
    ref_str: &str,
    components: Option<&ShimComponents>,
    media_type: &str,
    visiting: &mut std::collections::HashSet<String>,
) -> Option<serde_json::Value> {
    if !visiting.insert(ref_str.to_string()) {
        return None;
    }

    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let name = extract_component_name(ref_str, self_uri, "examples")?;
    let example_json = comps.extra.get("examples").and_then(|e| e.get(&name))?;
    extract_example_value(example_json, components, media_type, visiting)
}

fn resolve_example_from_components(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
) -> Option<serde_json::Value> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let ref_name = extract_component_name(&r.ref_location, self_uri, "examples")?;
    let example_json = comps.extra.get("examples").and_then(|e| e.get(&ref_name))?;
    let example = serde_json::from_value::<Example>(example_json.clone()).ok()?;
    example.value
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
) -> Option<serde_json::Value> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let name = extract_component_name(ref_str, self_uri, "schemas")?;
    let schema_json = comps.extra.get("schemas").and_then(|s| s.get(&name))?;
    extract_schema_example_from_value(schema_json)
}

fn raw_media_for_type(
    raw_body: &serde_json::Value,
    media_type: &str,
    components: Option<&ShimComponents>,
) -> Option<serde_json::Value> {
    let media = raw_body.get("content")?.get(media_type)?;
    resolve_media_type_ref(media, components, &mut HashSet::new()).or_else(|| {
        if media.as_object().is_some() {
            Some(media.clone())
        } else {
            None
        }
    })
}

fn resolve_media_type_ref(
    raw_media: &serde_json::Value,
    components: Option<&ShimComponents>,
    visiting: &mut HashSet<String>,
) -> Option<serde_json::Value> {
    let obj = raw_media.as_object()?;
    let ref_str = obj.get("$ref")?.as_str()?;
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let name = extract_component_name(ref_str, self_uri, "mediaTypes")?;
    if !visiting.insert(name.clone()) {
        return None;
    }
    let media_types = comps.extra.get("mediaTypes")?.as_object()?;
    let resolved = media_types.get(&name)?;
    let value =
        resolve_media_type_ref(resolved, components, visiting).unwrap_or_else(|| resolved.clone());
    visiting.remove(&name);
    Some(value)
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
            parsed.push(parse_encoding_value(item)?);
        }
        if !parsed.is_empty() {
            prefix_encoding = Some(parsed);
        }
    }

    let item_encoding = if let Some(item_val) = raw.get("itemEncoding") {
        Some(parse_encoding_value(item_val)?)
    } else {
        None
    };

    Ok((prefix_encoding, item_encoding))
}

fn parse_encoding_value(value: &serde_json::Value) -> AppResult<EncodingInfo> {
    let mut normalized = value.clone();
    if let serde_json::Value::Object(obj) = &mut normalized {
        obj.entry("headers".to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    }
    let enc = serde_json::from_value::<Encoding>(normalized).map_err(|e| {
        AppError::General(format!(
            "Failed to parse Encoding object from MediaType positional encoding: {}",
            e
        ))
    })?;
    encoding_to_info(&enc)
}

/// Helper to extract encoding map (property -> EncodingInfo).
fn extract_encoding_map(
    encoding: &std::collections::BTreeMap<String, Encoding>,
) -> AppResult<Option<HashMap<String, EncodingInfo>>> {
    if encoding.is_empty() {
        return Ok(None);
    }

    let mut map = HashMap::new();
    for (prop, enc) in encoding {
        map.insert(prop.clone(), encoding_to_info(enc)?);
    }

    if map.is_empty() {
        Ok(None)
    } else {
        Ok(Some(map))
    }
}

fn encoding_to_info(enc: &Encoding) -> AppResult<EncodingInfo> {
    let mut headers = HashMap::new();
    for (h_name, h_ref) in &enc.headers {
        let ty = map_schema_to_rust_type(&h_ref.schema, true)?;
        headers.insert(h_name.clone(), ty);
    }

    Ok(EncodingInfo {
        content_type: enc.content_type.clone(),
        headers,
        style: map_param_style(enc.style.as_ref()),
        explode: enc.explode,
        allow_reserved: enc.allow_reserved,
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
    use crate::oas::routes::shims::ShimComponents;
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
            .unwrap()
            .unwrap();
        assert_eq!(def.ty, "User");
        assert_eq!(def.media_type, "application/json");
        assert_eq!(def.format, BodyFormat::Json);
        assert!(!def.required);
        assert!(def.encoding.is_none());
    }

    #[test]
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
            .unwrap()
            .unwrap();
        assert_eq!(def.example, Some(json!({"id": 1, "name": "Ada"})));
    }

    #[test]
    fn test_extract_request_body_example_ref() {
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
            "examples".to_string(),
            json!({
                "BodyExample": {
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
                .unwrap()
                .unwrap();
        assert_eq!(def.example, Some(json!({"hello": "world"})));
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
            .unwrap()
            .unwrap();
        assert_eq!(def.example, Some(json!("https://example.com/example.json")));
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

        let body: ShimRequestBody = serde_json::from_value(raw).unwrap();
        let def = extract_request_body_type(&RefOr::T(body), None)
            .unwrap()
            .unwrap();
        assert_eq!(def.example, Some(json!({ "id": 42, "name": "Ada" })));
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

        let body: ShimRequestBody = serde_json::from_value(raw).unwrap();
        let def = extract_request_body_type(&RefOr::T(body), None)
            .unwrap()
            .unwrap();
        assert_eq!(def.example, Some(json!({ "id": 7, "name": "Grace" })));
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

        let body: ShimRequestBody = serde_json::from_value(raw).unwrap();
        let def = extract_request_body_type(&RefOr::T(body), None)
            .unwrap()
            .unwrap();
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

        let body: ShimRequestBody = serde_json::from_value(raw).unwrap();
        let def = extract_request_body_type(&RefOr::T(body), None)
            .unwrap()
            .unwrap();
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

        let body: ShimRequestBody = serde_json::from_value(raw).unwrap();
        let def = extract_request_body_type(&RefOr::T(body), None)
            .unwrap()
            .unwrap();
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

        let body: ShimRequestBody = serde_json::from_value(raw).unwrap();
        let def = extract_request_body_type(&RefOr::T(body), None).unwrap();
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

        let body: ShimRequestBody = serde_json::from_value(raw).unwrap();
        let err = extract_request_body_type(&RefOr::T(body), None).unwrap_err();
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
            .unwrap()
            .unwrap();

        assert_eq!(def.ty, "Upload");
        assert_eq!(def.media_type, "multipart/form-data");
        assert_eq!(def.format, BodyFormat::Multipart);

        let enc = def.encoding.unwrap();
        let profile = enc.get("profileImage").unwrap();
        assert_eq!(profile.content_type.as_deref(), Some("image/png"));
        assert_eq!(
            profile.headers.get("X-Image-Id").map(|s| s.as_str()),
            Some("Uuid")
        );

        let meta = enc.get("metadata").unwrap();
        assert_eq!(meta.content_type.as_deref(), Some("application/json"));
        assert!(meta.headers.is_empty());
        assert_eq!(meta.style, Some(crate::oas::models::ParamStyle::Form));
        assert_eq!(meta.explode, Some(false));
        assert_eq!(meta.allow_reserved, Some(true));
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

        let body: ShimRequestBody = serde_json::from_value(raw).unwrap();
        let def = extract_request_body_type(&RefOr::T(body), None)
            .unwrap()
            .unwrap();

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

        let body: ShimRequestBody = serde_json::from_value(raw).unwrap();
        let err = extract_request_body_type(&RefOr::T(body), None).unwrap_err();
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
            .unwrap()
            .unwrap();
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
            .unwrap()
            .unwrap();
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
            .unwrap()
            .unwrap();
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
            .unwrap()
            .unwrap();
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
            .unwrap()
            .unwrap();
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
            .unwrap()
            .unwrap();
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
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "__self".to_string(),
            json!("https://example.com/openapi.yaml"),
        );
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
            .unwrap()
            .unwrap();
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
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "__self".to_string(),
            json!("https://example.com/openapi.yaml"),
        );
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
            .unwrap()
            .unwrap();
        assert_eq!(def.description.as_deref(), Some("override"));
    }

    #[test]
    fn test_extract_request_body_media_type_ref() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
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
        let shim: ShimRequestBody = serde_json::from_value(body_json).unwrap();
        let def = extract_request_body_type(&RefOr::T(shim), Some(&components))
            .unwrap()
            .unwrap();
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
        let shim: ShimRequestBody = serde_json::from_value(body_json).unwrap();
        let def = extract_request_body_type(&RefOr::T(shim), None)
            .unwrap()
            .unwrap();
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
            .unwrap()
            .unwrap();
        assert!(def.required);
    }

    #[test]
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
            .unwrap()
            .unwrap();
        assert_eq!(def.example, Some(json!({"name": "Ada"})));
    }

    #[test]
    fn test_extract_request_body_schema_example_ref() {
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
                .unwrap()
                .unwrap();
        assert_eq!(def.example, Some(json!({"filename": "demo.png"})));
    }
}
