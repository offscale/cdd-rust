#![deny(missing_docs)]

//! # Response Resolution
//!
//! Logic for resolving OpenAPI Responses into Rust types.
//!
//! Additional validation:
//! - Link Objects must define exactly one of `operationId` or `operationRef`.
//! - Response headers support `content` and `$ref` resolution (OAS 3.2).
//! - Media type `$ref` entries under `content` resolve via `components.mediaTypes`.

use crate::error::{AppError, AppResult};
use crate::openapi::parse::models::{
    ExampleValue, LinkParamValue, LinkRequestBody, ParamStyle, ParsedLink, ParsedServer,
    ParsedServerVariable, ResponseHeader, RuntimeExpression,
};
use crate::openapi::parse::ref_utils::extract_component_name;
use crate::openapi::parse::registry::DocumentRegistry;
use crate::openapi::parse::resolver::types::{
    map_schema_to_rust_type, map_schema_to_rust_type_with_raw,
};
use crate::openapi::parse::routes::shims::{ShimComponents, ShimResponses};
use indexmap::IndexMap;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::collections::HashSet;
use url::Url;
use utoipa::openapi::{link::LinkBuilder, server::Server, Content, RefOr, Schema};

// Re-export specific structs if needed for external visibility
pub use crate::openapi::parse::models::ParsedLink as LinkModel;

/// The extracted details of a successful response.
#[derive(Debug)]
pub struct ParsedResponseDetails {
    /// The Rust type name of the body (if JSON).
    pub body_type: Option<String>,
    /// The chosen response status code or range (e.g., "200", "2XX", "default").
    pub status_code: Option<String>,
    /// The chosen response summary.
    pub summary: Option<String>,
    /// The chosen response description.
    pub description: Option<String>,
    /// The chosen response media type.
    pub media_type: Option<String>,
    /// Example payload for the response (data or serialized).
    pub example: Option<ExampleValue>,
    /// Extracted headers.
    pub headers: Vec<ResponseHeader>,
    /// Extracted links.
    pub links: Vec<ParsedLink>,
}

struct ResolvedResponse {
    response: utoipa::openapi::Response,
    raw: Option<JsonValue>,
    components: Option<ShimComponents>,
    base_uri: Option<Url>,
}

/// Extracts the success response type and its headers/links.
pub fn extract_response_details(
    responses: &ShimResponses,
    components: Option<&ShimComponents>,
) -> AppResult<Option<ParsedResponseDetails>> {
    extract_response_details_with_registry(responses, components, None, None)
}

/// Extracts the success response details with external reference resolution.
pub fn extract_response_details_with_registry(
    responses: &ShimResponses,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<Option<ParsedResponseDetails>> {
    let mut chosen_response = None;
    let mut chosen_key: Option<&str> = None;

    // 1. Direct Lookup Priorities
    let priorities = ["200", "201", "2XX", "2xx", "default", "3XX", "3xx"];
    for key in priorities {
        if let Some(r) = responses.inner.responses.get(key) {
            chosen_response = Some(r);
            chosen_key = Some(key);
            break;
        }
    }

    // 2. Fallback: Search for any concrete 2xx code
    if chosen_response.is_none() {
        for (key, resp) in &responses.inner.responses {
            if [
                key.len() == 3,
                key.starts_with('2'),
                key.chars().all(char::is_numeric),
            ]
            .into_iter()
            .all(|b| b)
            {
                chosen_response = Some(resp);
                chosen_key = Some(key.as_str());
                break;
            }
        }
    }

    if let Some(resp_item) = chosen_response {
        let mut summary_override: Option<String> = None;
        let mut override_components: Option<ShimComponents> = None;
        let mut override_base: Option<Url> = None;
        let (response, raw_response) = match resp_item {
            RefOr::T(val) => (
                Some(val.clone()),
                chosen_key.and_then(|key| responses.raw.get(key)).cloned(),
            ),
            RefOr::Ref(r) => {
                let resolved = resolve_response_from_components(r, components, registry, base_uri);
                if let Some(resolved) = resolved {
                    let mut resp = resolved.response;
                    if !r.summary.is_empty() {
                        summary_override = Some(r.summary.clone());
                    }
                    if !r.description.is_empty() {
                        resp.description = r.description.clone();
                    }
                    override_components = resolved.components;
                    override_base = resolved.base_uri;
                    (Some(resp), resolved.raw)
                } else {
                    (None, None)
                }
            }
        };

        let components_ctx = override_components.as_ref().or(components);
        let base_ctx = override_base.as_ref().or(base_uri);

        if let Some(r) = response {
            let mut selected_media_type = None;
            let mut body_type = None;
            let mut example = None;

            if let Some((media_type, media)) = select_response_media(&r.content) {
                selected_media_type = Some(media_type.to_string());

                let raw_media = raw_response.as_ref().and_then(|raw| {
                    raw_media_for_type(raw, media_type, components_ctx, registry, base_ctx)
                });

                example = crate::openapi::parse::resolver::body::extract_media_example(
                    media,
                    components_ctx,
                    registry,
                    base_ctx,
                    raw_media.as_ref(),
                    media_type,
                );

                let raw_schema_json = raw_media.as_ref().and_then(|raw| raw.get("schema"));
                if let Some(schema) = media.schema.as_ref() {
                    body_type = Some(map_schema_to_rust_type_with_raw(
                        schema,
                        true,
                        raw_schema_json,
                    )?);
                } else if raw_schema_is_false(raw_media.as_ref(), "schema")
                    || raw_schema_is_false(raw_media.as_ref(), "itemSchema")
                {
                    body_type = None;
                } else if let Some(schema_ref) = raw_media.as_ref().and_then(extract_media_schema) {
                    body_type = Some(map_schema_to_rust_type_with_raw(
                        &schema_ref,
                        true,
                        raw_schema_json,
                    )?);
                } else if let Some(item_schema) = raw_media.as_ref().and_then(extract_item_schema) {
                    let inner = map_schema_to_rust_type(&item_schema, true)?;
                    if is_sequential_media_type(&normalize_media_type(media_type)) {
                        body_type = Some(format!("Vec<{}>", inner));
                    } else {
                        body_type = Some(inner);
                    }
                } else {
                    body_type = infer_body_type_from_media_type(media_type);
                }
            }

            // 2. Resolve Headers
            let headers = extract_response_headers(
                &r,
                raw_response.as_ref(),
                components_ctx,
                registry,
                base_ctx,
            )?;

            // 3. Resolve Links
            let mut links = Vec::new();
            if !r.links.is_empty() {
                for (name, link_val) in &r.links {
                    let link_obj = match link_val {
                        RefOr::T(l) => Some(l.clone()),
                        RefOr::Ref(r) => {
                            resolve_link_from_ref(r, components_ctx, registry, base_ctx)
                        }
                    };

                    if let Some(l) = link_obj {
                        let has_op_id = !l.operation_id.is_empty();
                        let has_op_ref = !l.operation_ref.is_empty();
                        if has_op_id == has_op_ref {
                            return Err(AppError::General(format!(
                                "Link '{}' must define exactly one of 'operationId' or 'operationRef'",
                                name
                            )));
                        }
                        let description = if l.description.is_empty() {
                            None
                        } else {
                            Some(l.description.clone())
                        };
                        let operation_id = if l.operation_id.is_empty() {
                            None
                        } else {
                            Some(l.operation_id.clone())
                        };
                        let operation_ref = if l.operation_ref.is_empty() {
                            None
                        } else {
                            Some(l.operation_ref.clone())
                        };

                        let parameters = parse_link_parameters(&l.parameters, name)?;
                        let request_body = parse_link_request_body(&l.request_body)?;

                        let mut server = None;
                        let mut server_url = None;
                        if let Some(s) = l.server.as_ref() {
                            let parsed = parsed_server_from_utoipa(s);
                            server_url = Some(resolve_server_url_from_parsed(&parsed));
                            server = Some(parsed);
                        }

                        links.push(ParsedLink {
                            name: name.clone(),
                            description,
                            operation_id,
                            operation_ref,
                            resolved_operation_ref: None,
                            parameters,
                            request_body,
                            server,
                            server_url,
                        });
                    }
                }
            } else if let Some(raw_links) = raw_response.as_ref().and_then(|raw| raw.get("links")) {
                links = extract_links_from_raw(raw_links, components_ctx, registry, base_ctx)?;
            }

            let raw_summary = raw_response
                .as_ref()
                .and_then(|raw| raw.get("summary"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let summary = summary_override.or(raw_summary);

            let description = if r.description.trim().is_empty() {
                None
            } else {
                Some(r.description.clone())
            };
            let status_code = chosen_key.map(|key| key.to_string());

            return Ok(Some(ParsedResponseDetails {
                body_type,
                status_code,
                summary,
                description,
                media_type: selected_media_type,
                example,
                headers,
                links,
            }));
        }
    }

    Ok(None)
}

fn parsed_server_from_utoipa(server: &Server) -> ParsedServer {
    let mut variables = BTreeMap::new();
    if let Some(vars) = server.variables.as_ref() {
        for (name, var) in vars {
            variables.insert(
                name.clone(),
                ParsedServerVariable {
                    enum_values: var.enum_values.clone(),
                    default: var.default_value.clone(),
                    description: var.description.clone(),
                },
            );
        }
    }

    ParsedServer {
        url: server.url.clone(),
        description: server.description.clone(),
        name: None,
        variables,
    }
}

fn resolve_server_url_from_parsed(server: &ParsedServer) -> String {
    let mut url = server.url.clone();
    for (name, var) in &server.variables {
        let placeholder = format!("{{{}}}", name);
        url = url.replace(&placeholder, &var.default);
    }
    url
}

fn raw_schema_is_false(raw_media: Option<&JsonValue>, key: &str) -> bool {
    matches!(
        raw_media
            .and_then(|raw| raw.get(key))
            .and_then(|v| v.as_bool()),
        Some(false)
    )
}

fn filter_header_extensions(
    obj: &serde_json::Map<String, JsonValue>,
) -> BTreeMap<String, JsonValue> {
    obj.iter()
        .filter(|(key, _)| key.starts_with("x-"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn extract_response_headers(
    response: &utoipa::openapi::Response,
    raw_response: Option<&JsonValue>,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<Vec<ResponseHeader>> {
    if let Some(raw_headers) = raw_response
        .and_then(|raw| raw.get("headers"))
        .and_then(|headers| headers.as_object())
    {
        return extract_headers_from_raw(raw_headers, components, registry, base_uri);
    }

    // Fallback to typed headers when raw headers are not available.
    let mut headers = Vec::new();
    for (name, header_ref) in &response.headers {
        if name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        if let RefOr::T(header_obj) = header_ref {
            let ty = match &header_obj.schema {
                Some(schema) => map_schema_to_rust_type(schema, true)?,
                None => "String".to_string(),
            };
            headers.push(ResponseHeader {
                name: name.clone(),
                description: header_obj.description.clone(),
                required: false,
                deprecated: false,
                style: None,
                explode: None,
                ty,
                content_media_type: None,
                example: None,
                extensions: BTreeMap::new(),
            });
        }
    }
    Ok(headers)
}

/// Extracts header definitions from raw response JSON so OAS 3.2 `content` is honored.
fn extract_headers_from_raw(
    raw_headers: &serde_json::Map<String, JsonValue>,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<Vec<ResponseHeader>> {
    let mut headers = Vec::new();
    for (name, raw_header) in raw_headers {
        if name.eq_ignore_ascii_case("content-type") {
            continue;
        }

        let mut visited = HashSet::new();
        let resolved =
            resolve_header_value(raw_header, components, registry, base_uri, &mut visited)?;
        let Some(obj) = resolved.as_object() else {
            return Err(AppError::General(format!(
                "Header '{}' must be an object",
                name
            )));
        };

        let description = obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut content_media_type = None;
        let mut example = None;
        let required = obj
            .get("required")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let deprecated = obj
            .get("deprecated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let has_content = obj.get("content").is_some();
        let style = parse_header_style(name, obj.get("style").and_then(|v| v.as_str()))?;
        let explode = obj.get("explode").and_then(|v| v.as_bool());
        if has_content && [style.is_some(), explode.is_some()].into_iter().any(|b| b) {
            return Err(AppError::General(format!(
                "Header '{}' uses 'content' and must not define style or explode",
                name
            )));
        }

        let ty = if let Some(content_val) = obj.get("content") {
            let (ty, media_type, media_example) =
                extract_header_content_info(name, content_val, components, registry, base_uri)?;
            content_media_type = Some(media_type);
            example = media_example;
            ty
        } else if let Some(schema_val) = obj.get("schema") {
            example = extract_header_example_from_schema(obj, components);
            header_schema_to_type(name, schema_val)?
        } else {
            "String".to_string()
        };

        headers.push(ResponseHeader {
            name: name.clone(),
            description,
            required,
            deprecated,
            style,
            explode,
            ty,
            content_media_type,
            example,
            extensions: filter_header_extensions(obj),
        });
    }

    Ok(headers)
}

/// Resolves a header type and media type when using `content`.
fn extract_header_content_info(
    name: &str,
    content_val: &JsonValue,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<(String, String, Option<ExampleValue>)> {
    let Some(content_map) = content_val.as_object() else {
        return Err(AppError::General(format!(
            "Header '{}' content must be an object",
            name
        )));
    };

    if content_map.len() != 1 {
        return Err(AppError::General(format!(
            "Header '{}' content must define exactly one media type",
            name
        )));
    }

    let (media_type, media_obj) = content_map.iter().next().expect("Failed to next");
    let resolved = resolve_media_type_ref(
        media_obj,
        components,
        registry,
        base_uri,
        &mut HashSet::new(),
    )
    .unwrap_or_else(|| media_obj.clone());
    let Some(resolved_obj) = resolved.as_object() else {
        return Err(AppError::General(format!(
            "Header '{}' content must be an object",
            name
        )));
    };

    if let Some(schema_val) = resolved_obj.get("schema") {
        let ty = header_schema_to_type(name, schema_val)?;
        let example = extract_header_example_from_media(&resolved, components, media_type);
        return Ok((ty, media_type.to_string(), example));
    }
    if let Some(item_schema) = resolved_obj.get("itemSchema") {
        let schema: RefOr<Schema> = serde_json::from_value(item_schema.clone()).map_err(|e| {
            AppError::General(format!(
                "Failed to parse header '{}' itemSchema: {}",
                name, e
            ))
        })?;
        let inner = map_schema_to_rust_type(&schema, true)?;
        let ty = if is_sequential_media_type(&normalize_media_type(media_type)) {
            format!("Vec<{}>", inner)
        } else {
            inner
        };
        let example = extract_header_example_from_media(&resolved, components, media_type);
        return Ok((ty, media_type.to_string(), example));
    }

    let ty = infer_body_type_from_media_type(media_type).unwrap_or_else(|| "String".to_string());
    let example = extract_header_example_from_media(&resolved, components, media_type);
    Ok((ty, media_type.to_string(), example))
}

fn extract_header_example_from_schema(
    obj: &serde_json::Map<String, JsonValue>,
    components: Option<&ShimComponents>,
) -> Option<ExampleValue> {
    if let Some(example) = obj.get("example") {
        return Some(ExampleValue::data(example.clone()));
    }

    let examples = obj.get("examples").and_then(|v| v.as_object())?;
    let mut visiting = HashSet::new();
    for value in examples.values() {
        if let Some(example) = crate::openapi::parse::resolver::body::extract_example_value(
            value,
            components,
            "text/plain",
            &mut visiting,
        ) {
            return Some(example);
        }
    }

    None
}

fn extract_header_example_from_media(
    media_val: &JsonValue,
    components: Option<&ShimComponents>,
    media_type: &str,
) -> Option<ExampleValue> {
    let obj = media_val.as_object()?;
    if let Some(example) = obj.get("example") {
        return Some(ExampleValue::data(example.clone()));
    }
    let examples = obj.get("examples").and_then(|v| v.as_object())?;
    let mut visiting = HashSet::new();
    for value in examples.values() {
        if let Some(example) = crate::openapi::parse::resolver::body::extract_example_value(
            value,
            components,
            media_type,
            &mut visiting,
        ) {
            return Some(example);
        }
    }
    None
}

/// Resolves a header that may be a `$ref`, applying description overrides.
fn resolve_header_value(
    raw_header: &JsonValue,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
    visited: &mut HashSet<String>,
) -> AppResult<JsonValue> {
    let Some(obj) = raw_header.as_object() else {
        return Ok(raw_header.clone());
    };

    let Some(ref_str) = obj.get("$ref").and_then(|v| v.as_str()) else {
        return Ok(raw_header.clone());
    };

    if !visited.insert(ref_str.to_string()) {
        return Err(AppError::General(format!(
            "Header reference cycle detected at '{}'",
            ref_str
        )));
    }

    let mut resolved = resolve_header_from_components(ref_str, components, registry, base_uri)
        .ok_or_else(|| {
            AppError::General(format!(
                "Header reference '{}' could not be resolved",
                ref_str
            ))
        })?;
    resolved = resolve_header_value(&resolved, components, registry, base_uri, visited)?;

    if let (Some(desc), Some(resolved_obj)) = (
        obj.get("description").and_then(|v| v.as_str()),
        resolved.as_object_mut(),
    ) {
        resolved_obj.insert(
            "description".to_string(),
            JsonValue::String(desc.to_string()),
        );
    }

    visited.remove(ref_str);
    Ok(resolved)
}

fn header_schema_to_type(name: &str, schema_val: &JsonValue) -> AppResult<String> {
    if let Some(flag) = schema_val.as_bool() {
        if !flag {
            return Err(AppError::General(format!(
                "Header '{}' schema is 'false' and cannot be satisfied",
                name
            )));
        }
        return Ok("String".to_string());
    }

    let schema: RefOr<Schema> = serde_json::from_value(schema_val.clone()).map_err(|e| {
        AppError::General(format!(
            "Failed to parse header schema for '{}': {}",
            name, e
        ))
    })?;
    map_schema_to_rust_type_with_raw(&schema, true, Some(schema_val))
}

fn parse_header_style(name: &str, style: Option<&str>) -> AppResult<Option<ParamStyle>> {
    let Some(style_str) = style else {
        return Ok(None);
    };

    let parsed = match style_str {
        "simple" => ParamStyle::Simple,
        other => {
            return Err(AppError::General(format!(
                "Header '{}' uses style '{}' which is not allowed for headers",
                name, other
            )))
        }
    };

    Ok(Some(parsed))
}

/// Resolves a header component reference to its raw JSON definition.
fn resolve_header_from_components(
    ref_str: &str,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<JsonValue> {
    if let Some(header) = components.and_then(|c| {
        let name = extract_component_name(ref_str, c.self_uri.as_deref(), "headers")?;
        c.extra.get("headers")?.get(&name).cloned()
    }) {
        return Some(header);
    }

    registry
        .and_then(|reg| reg.resolve_component_ref_with_components(ref_str, base_uri, "headers"))
        .map(|(raw, _, _)| raw)
}

fn raw_media_for_type(
    raw_response: &serde_json::Value,
    media_type: &str,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<serde_json::Value> {
    let media = raw_response.get("content")?.as_object()?.get(media_type)?;
    resolve_media_type_ref(media, components, registry, base_uri, &mut HashSet::new())
        .or_else(|| media.as_object().map(|_| media.clone()))
}

fn extract_item_schema(raw_media: &serde_json::Value) -> Option<RefOr<Schema>> {
    let item = raw_media.get("itemSchema")?.clone();
    serde_json::from_value::<RefOr<Schema>>(item).ok()
}

fn extract_media_schema(raw_media: &serde_json::Value) -> Option<RefOr<Schema>> {
    let schema_val = raw_media.get("schema")?.clone();
    serde_json::from_value::<RefOr<Schema>>(schema_val).ok()
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

/// Selects the most appropriate response content for JSON-like payloads.
///
/// Preference order:
/// 1. `application/json`
/// 2. Any `+json` media type (e.g. `application/vnd.api+json`)
/// 3. `application/*`
/// 4. `*/*`
fn empty_content() -> &'static Content {
    static EMPTY: std::sync::LazyLock<Content> = std::sync::LazyLock::new(Content::default);
    &EMPTY
}

/// 5. First available entry
fn select_response_media(content: &IndexMap<String, RefOr<Content>>) -> Option<(&str, &Content)> {
    if let Some((key, media)) =
        select_best_media(content, is_json_media_type, Some("application/json"))
    {
        return Some((key, media));
    }

    if let Some((key, media)) = select_best_media(content, is_text_media_type, Some("text/plain")) {
        return Some((key, media));
    }

    if let Some((key, media)) = select_best_media(
        content,
        |k| normalize_media_type(k) == "application/*" || normalize_media_type(k) == "*/*",
        Some("application/*"),
    ) {
        return Some((key, media));
    }

    content.iter().next().map(|(k, v)| match v {
        RefOr::T(media) => (k.as_str(), media),
        RefOr::Ref(_) => (k.as_str(), empty_content()),
    })
}

fn select_best_media<'a, F>(
    content: &'a IndexMap<String, RefOr<Content>>,
    predicate: F,
    preferred: Option<&str>,
) -> Option<(&'a str, &'a Content)>
where
    F: Fn(&str) -> bool,
{
    let filtered = content.iter().filter(|(k, _)| predicate(k));
    let (k, v) = filtered.max_by_key(|(k, _)| media_specificity_score(k, preferred))?;
    let media = match v {
        RefOr::T(m) => m,
        RefOr::Ref(_) => empty_content(),
    };
    Some((k.as_str(), media))
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

fn infer_body_type_from_media_type(media_type: &str) -> Option<String> {
    let media = normalize_media_type(media_type);

    if is_sequential_json_media_type(&media) {
        return Some("Vec<serde_json::Value>".to_string());
    }

    if is_json_media_type(&media) {
        return Some("serde_json::Value".to_string());
    }

    if is_text_media_type(&media) {
        return Some("String".to_string());
    }

    if media == "application/octet-stream"
        || media == "application/pdf"
        || media.starts_with("image/")
        || media.starts_with("audio/")
        || media.starts_with("video/")
    {
        return Some("Vec<u8>".to_string());
    }

    None
}

fn normalize_media_type(media_type: &str) -> String {
    crate::openapi::parse::models::MediaType::normalize(media_type)
}

fn is_json_media_type(media_type: &str) -> bool {
    crate::openapi::parse::models::MediaType::parse(media_type).is_json()
}

fn is_text_media_type(media_type: &str) -> bool {
    crate::openapi::parse::models::MediaType::parse(media_type).is_text()
}

fn is_sequential_json_media_type(media_type: &str) -> bool {
    crate::openapi::parse::models::MediaType::parse(media_type).is_sequential_json()
}

fn is_sequential_media_type(media_type: &str) -> bool {
    if is_sequential_json_media_type(media_type) {
        return true;
    }

    matches!(
        media_type,
        "text/event-stream" | "multipart/mixed" | "multipart/byteranges"
    )
}

fn resolve_response_from_components(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<ResolvedResponse> {
    if let Some((resp, raw)) = components.and_then(|c| {
        let name = extract_component_name(&r.ref_location, c.self_uri.as_deref(), "responses")?;
        let resp_json = c.extra.get("responses")?.get(&name)?;
        let resp = serde_json::from_value::<utoipa::openapi::Response>(resp_json.clone()).ok()?;
        Some((resp, resp_json.clone()))
    }) {
        return Some(ResolvedResponse {
            response: resp,
            raw: Some(raw),
            components: None,
            base_uri: None,
        });
    }

    registry
        .and_then(|reg| {
            reg.resolve_component_ref_with_components(&r.ref_location, base_uri, "responses")
        })
        .and_then(|(raw, comps_override, base_override)| {
            let resp = serde_json::from_value::<utoipa::openapi::Response>(raw.clone()).ok()?;
            Some(ResolvedResponse {
                response: resp,
                raw: Some(raw),
                components: comps_override,
                base_uri: base_override,
            })
        })
}

fn resolve_link_from_ref(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<utoipa::openapi::link::Link> {
    let raw = resolve_link_raw_from_components(&r.ref_location, components, registry, base_uri)?;
    link_from_component_json(&raw, &r.description)
}

fn normalize_link_object(value: &mut JsonValue) {
    let JsonValue::Object(link_obj) = value else {
        return;
    };

    for (snake, camel) in [
        ("operation_id", "operationId"),
        ("operation_ref", "operationRef"),
        ("request_body", "requestBody"),
    ] {
        if !link_obj.contains_key(snake) {
            if let Some(val) = link_obj.remove(camel) {
                link_obj.insert(snake.to_string(), val);
            }
        }
    }
}

fn link_from_component_json(
    value: &JsonValue,
    description_override: &str,
) -> Option<utoipa::openapi::link::Link> {
    let mut normalized = value.clone();
    normalize_link_object(&mut normalized);
    let mut link = serde_json::from_value::<utoipa::openapi::link::Link>(normalized.clone())
        .ok()
        .or_else(|| link_from_raw(&normalized))
        .expect("normalized object produces Link");
    if !description_override.is_empty() {
        link.description = description_override.to_string();
    }
    Some(link)
}

fn link_from_raw(value: &JsonValue) -> Option<utoipa::openapi::link::Link> {
    let obj = value.as_object()?;
    let operation_id = obj
        .get("operation_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let operation_ref = obj
        .get("operation_ref")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let parameters: BTreeMap<String, serde_json::Value> = obj
        .get("parameters")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let request_body = obj.get("request_body").cloned();
    let server: Option<Server> = obj
        .get("server")
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    let mut builder = LinkBuilder::new();
    if !operation_id.is_empty() {
        builder = builder.operation_id(operation_id);
    }
    if !operation_ref.is_empty() {
        builder = builder.operation_ref(operation_ref);
    }
    if !description.is_empty() {
        builder = builder.description(description);
    }
    for (name, value) in parameters {
        builder = builder.parameter(name, value);
    }
    builder = builder.request_body(request_body);
    builder = builder.server(server);
    Some(builder.build())
}

fn resolve_link_raw_from_components(
    ref_str: &str,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<JsonValue> {
    if let Some(link) = components.and_then(|c| {
        let name = extract_component_name(ref_str, c.self_uri.as_deref(), "links")?;
        c.extra.get("links")?.get(&name).cloned()
    }) {
        return Some(link);
    }

    registry
        .and_then(|reg| reg.resolve_component_ref_with_components(ref_str, base_uri, "links"))
        .map(|(raw, _, _)| raw)
}

fn resolve_link_value(
    raw_link: &JsonValue,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
    visited: &mut HashSet<String>,
) -> AppResult<JsonValue> {
    let Some(obj) = raw_link.as_object() else {
        return Ok(raw_link.clone());
    };

    let Some(ref_str) = obj.get("$ref").and_then(|v| v.as_str()) else {
        return Ok(raw_link.clone());
    };

    if !visited.insert(ref_str.to_string()) {
        return Err(AppError::General(format!(
            "Link reference cycle detected at '{}'",
            ref_str
        )));
    }

    let mut resolved = resolve_link_raw_from_components(ref_str, components, registry, base_uri)
        .ok_or_else(|| {
            AppError::General(format!(
                "Link reference '{}' could not be resolved",
                ref_str
            ))
        })?;
    resolved = resolve_link_value(&resolved, components, registry, base_uri, visited)?;

    for key in ["description", "summary"] {
        if let (Some(val), Some(resolved_obj)) = (obj.get(key), resolved.as_object_mut()) {
            resolved_obj.insert(key.to_string(), val.clone());
        }
    }

    visited.remove(ref_str);
    Ok(resolved)
}

fn parse_server_from_raw(server: &JsonValue) -> Option<ParsedServer> {
    let obj = server.as_object()?;
    let url = obj.get("url")?.as_str()?.to_string();
    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut variables = BTreeMap::new();
    for (var_name, var) in obj
        .get("variables")
        .and_then(|v| v.as_object())
        .into_iter()
        .flatten()
    {
        let Some(var_obj) = var.as_object() else {
            continue;
        };
        let Some(default) = var_obj.get("default").and_then(|v| v.as_str()) else {
            continue;
        };
        let enum_values = var_obj.get("enum").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        });
        let description = var_obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        variables.insert(
            var_name.clone(),
            ParsedServerVariable {
                enum_values,
                default: default.to_string(),
                description,
            },
        );
    }

    Some(ParsedServer {
        url,
        description,
        name,
        variables,
    })
}

fn parse_link_parameters(
    parameters: &BTreeMap<String, serde_json::Value>,
    link_name: &str,
) -> AppResult<std::collections::HashMap<String, LinkParamValue>> {
    let mut parsed = std::collections::HashMap::new();

    for (key, value) in parameters {
        validate_link_parameter_key(link_name, key)?;
        if let Some(expr) = runtime_expr_from_json(value)? {
            parsed.insert(key.clone(), LinkParamValue::Expression(expr));
        } else {
            parsed.insert(key.clone(), LinkParamValue::Literal(value.clone()));
        }
    }

    Ok(parsed)
}

fn parse_link_request_body(
    request_body: &Option<serde_json::Value>,
) -> AppResult<Option<LinkRequestBody>> {
    let Some(body) = request_body else {
        return Ok(None);
    };

    if let Some(expr) = runtime_expr_from_json(body)? {
        return Ok(Some(LinkRequestBody::Expression(expr)));
    }

    Ok(Some(LinkRequestBody::Literal(body.clone())))
}

fn extract_links_from_raw(
    raw_links: &JsonValue,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<Vec<ParsedLink>> {
    let Some(map) = raw_links.as_object() else {
        return Ok(Vec::new());
    };

    let mut links = Vec::new();
    for (name, link_val) in map {
        let mut visited = HashSet::new();
        let resolved = resolve_link_value(link_val, components, registry, base_uri, &mut visited)?;
        let Some(obj) = resolved.as_object() else {
            return Err(AppError::General(format!(
                "Link '{}' must be an object",
                name
            )));
        };

        let description = obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let operation_id = obj
            .get("operationId")
            .or_else(|| obj.get("operation_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let operation_ref = obj
            .get("operationRef")
            .or_else(|| obj.get("operation_ref"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if operation_id.is_some() == operation_ref.is_some() {
            return Err(AppError::General(format!(
                "Link '{}' must define exactly one of 'operationId' or 'operationRef'",
                name
            )));
        }

        let parameters = if let Some(params_val) = obj.get("parameters") {
            let params: BTreeMap<String, JsonValue> = serde_json::from_value(params_val.clone())
                .map_err(|e| {
                    AppError::General(format!(
                        "Link '{}' parameters must be an object: {}",
                        name, e
                    ))
                })?;
            parse_link_parameters(&params, name)?
        } else {
            std::collections::HashMap::new()
        };

        let request_body = parse_link_request_body(
            &obj.get("requestBody")
                .or_else(|| obj.get("request_body"))
                .cloned(),
        )?;
        let mut server = None;
        let mut server_url = None;
        if let Some(server_val) = obj.get("server") {
            if let Some(parsed) = parse_server_from_raw(server_val) {
                server_url = Some(resolve_server_url_from_parsed(&parsed));
                server = Some(parsed);
            }
        }

        links.push(ParsedLink {
            name: name.clone(),
            description,
            operation_id,
            operation_ref,
            resolved_operation_ref: None,
            parameters,
            request_body,
            server,
            server_url,
        });
    }

    Ok(links)
}

fn runtime_expr_from_json(value: &serde_json::Value) -> AppResult<Option<RuntimeExpression>> {
    let candidate = match value {
        serde_json::Value::String(s) => s.as_str(),
        _ => return Ok(None),
    };

    let expr = RuntimeExpression::parse(candidate)?;
    if expr.is_expression() {
        return Ok(Some(expr));
    }

    Ok(None)
}

fn validate_link_parameter_key(link_name: &str, key: &str) -> AppResult<()> {
    if let Some((prefix, rest)) = key.split_once('.') {
        if matches!(prefix, "path" | "query" | "header" | "cookie") && rest.is_empty() {
            return Err(AppError::General(format!(
                "Link '{}' parameter key '{}' must include a parameter name after '{}.'",
                link_name, key, prefix
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::routes::shims::ShimComponents;
    use crate::openapi::parse::routes::shims::ShimResponses;
    use serde_json::json;
    use std::collections::BTreeMap;
    use utoipa::openapi::{
        header::HeaderBuilder,
        link::LinkBuilder,
        schema::{ObjectBuilder, Type},
        server::ServerBuilder,
        server::ServerVariableBuilder,
        Content, ResponseBuilder, Responses, Schema, Server,
    };

    fn wrap(responses: Responses) -> ShimResponses {
        ShimResponses::from(responses)
    }

    #[test]
    fn test_extract_inline_response() {
        let response = ResponseBuilder::new()
            .description("Inline")
            .content(
                "application/json",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/User",
                )))),
            )
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.expect("Missing body type"), "User");
    }

    #[test]
    fn test_extract_response_metadata_status_description_media_type() {
        let response = ResponseBuilder::new()
            .description("Created")
            .content(
                "text/plain",
                Content::new(Some(RefOr::T(Schema::Object(
                    ObjectBuilder::new().schema_type(Type::String).build(),
                )))),
            )
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("201".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.status_code.as_deref(), Some("201"));
        assert_eq!(details.description.as_deref(), Some("Created"));
        assert_eq!(details.media_type.as_deref(), Some("text/plain"));
    }

    #[test]
    fn test_extract_response_example_data_value() {
        let raw = json!({
            "200": {
                "description": "OK",
                "content": {
                    "application/json": {
                        "schema": { "type": "object", "properties": { "id": { "type": "integer" } } },
                        "examples": {
                            "good": { "dataValue": { "id": 42 } }
                        }
                    }
                }
            }
        });
        let responses = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&responses, None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        let example = details.example.expect("example");
        assert!(!example.is_serialized());
        assert_eq!(example.value, json!({ "id": 42 }));
    }

    #[test]
    fn test_extract_response_example_serialized_value() {
        let raw = json!({
            "200": {
                "description": "OK",
                "content": {
                    "text/plain": {
                        "examples": {
                            "plain": { "serializedValue": "hello" }
                        }
                    }
                }
            }
        });
        let responses = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&responses, None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        let example = details.example.expect("example");
        assert!(example.is_serialized());
        assert_eq!(example.value, json!("hello"));
    }

    #[test]
    fn test_extract_response_ndjson_default_type() {
        let response = ResponseBuilder::new()
            .description("NDJSON")
            .content("application/x-ndjson", Content::new::<RefOr<Schema>>(None))
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.as_deref(), Some("Vec<serde_json::Value>"));
    }

    #[test]
    fn test_select_response_media_prefers_specific_text() {
        let text_any = Content::new(Some(RefOr::T(Schema::Object(
            ObjectBuilder::new().schema_type(Type::Integer).build(),
        ))));
        let text_plain = Content::new(Some(RefOr::T(Schema::Object(
            ObjectBuilder::new().schema_type(Type::String).build(),
        ))));

        let response = ResponseBuilder::new()
            .description("Text")
            .content("text/*", text_any)
            .content("text/plain", text_plain)
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.as_deref(), Some("String"));
    }

    #[test]
    fn test_select_response_media_prefers_application_json() {
        let vendor_json = Content::new(Some(RefOr::T(Schema::Object(
            ObjectBuilder::new().schema_type(Type::Integer).build(),
        ))));
        let app_json = Content::new(Some(RefOr::T(Schema::Object(
            ObjectBuilder::new().schema_type(Type::String).build(),
        ))));

        let response = ResponseBuilder::new()
            .description("Json")
            .content("application/vnd.api+json", vendor_json)
            .content("application/json", app_json)
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.as_deref(), Some("String"));
    }

    #[test]
    fn test_extract_response_ndjson_item_schema_type() {
        let raw = json!({
            "200": {
                "description": "NDJSON",
                "content": {
                    "application/x-ndjson": {
                        "itemSchema": { "type": "string" }
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&shim, None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.as_deref(), Some("Vec<String>"));
    }

    #[test]
    fn test_extract_response_vendor_jsonl_item_schema_type() {
        let raw = json!({
            "200": {
                "description": "Vendor JSONL",
                "content": {
                    "application/vnd.acme+jsonl": {
                        "itemSchema": { "type": "string" }
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&shim, None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.as_deref(), Some("Vec<String>"));
    }

    #[test]
    fn test_extract_response_event_stream_item_schema_type() {
        let raw = json!({
            "200": {
                "description": "SSE",
                "content": {
                    "text/event-stream": {
                        "itemSchema": { "type": "object" }
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&shim, None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.as_deref(), Some("Vec<serde_json::Value>"));
    }

    #[test]
    fn test_extract_response_summary_from_raw() {
        let raw = json!({
            "200": {
                "summary": "Accepted payload",
                "description": "OK"
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&shim, None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.summary.as_deref(), Some("Accepted payload"));
    }

    #[test]
    fn test_extract_response_multipart_item_schema_type() {
        let raw = json!({
            "200": {
                "description": "Multipart",
                "content": {
                    "multipart/mixed": {
                        "itemSchema": { "type": "string" }
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&shim, None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.as_deref(), Some("Vec<String>"));
    }

    #[test]
    fn test_extract_response_schema_false_skips_body() {
        let raw = json!({
            "200": {
                "description": "No body",
                "content": {
                    "application/json": {
                        "schema": false
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&shim, None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert!(details.body_type.is_none());
    }

    #[test]
    fn test_extract_response_content_schema_mapping() {
        let raw = json!({
            "200": {
                "description": "Content schema",
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
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&shim, None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.as_deref(), Some("i32"));
    }

    #[test]
    fn test_link_server_variable_resolution() {
        let server = ServerBuilder::new()
            .url("https://{region}.example.com/{basePath}")
            .parameter(
                "region",
                ServerVariableBuilder::new().default_value("us-east-1"),
            )
            .parameter("basePath", ServerVariableBuilder::new().default_value("v1"))
            .build();

        let link = LinkBuilder::new()
            .operation_id("getUser")
            .server(Some(server))
            .build();

        let response = ResponseBuilder::new()
            .description("OK")
            .link("User", link)
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        let link = details.links.first().expect("expected link");
        assert_eq!(
            link.server_url.as_deref(),
            Some("https://us-east-1.example.com/v1")
        );
        let server = link.server.as_ref().expect("expected link server");
        assert_eq!(server.url, "https://{region}.example.com/{basePath}");
        assert_eq!(server.variables["region"].default, "us-east-1");
        assert_eq!(server.variables["basePath"].default, "v1");
    }

    #[test]
    fn test_extract_response_xml_default_type() {
        let response = ResponseBuilder::new()
            .description("XML")
            .content("application/xml", Content::new::<RefOr<Schema>>(None))
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.as_deref(), Some("String"));
    }

    #[test]
    fn test_extract_response_links_with_runtime_expression() {
        let link = LinkBuilder::new()
            .operation_id("getUser")
            .description("Get user")
            .parameter("userId", "$request.path.id")
            .build();

        let response = ResponseBuilder::new()
            .description("Linked Response")
            .link("UserLink", link)
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        let l = &details.links[0];
        let expr = l.parameters.get("userId").expect("Failed to get");

        assert_eq!(
            expr,
            &LinkParamValue::Expression(RuntimeExpression::new("$request.path.id"))
        );
    }

    #[test]
    fn test_extract_response_links_with_runtime_expression_template() {
        let link = LinkBuilder::new()
            .operation_id("getUser")
            .description("Get user")
            .parameter("userId", "id={$request.path.id}")
            .build();

        let response = ResponseBuilder::new()
            .description("Linked Response")
            .link("UserLink", link)
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        let l = &details.links[0];
        let expr = l.parameters.get("userId").expect("Failed to get");

        assert_eq!(
            expr,
            &LinkParamValue::Expression(RuntimeExpression::new("id={$request.path.id}"))
        );
    }

    #[test]
    fn test_extract_response_links_with_snake_case_keys() {
        let raw = json!({
            "200": {
                "description": "ok",
                "links": {
                    "Self": {
                        "operation_id": "getUser",
                        "request_body": "$response.body#/id"
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&shim, None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        let link = &details.links[0];
        assert_eq!(link.operation_id.as_deref(), Some("getUser"));
        assert_eq!(
            link.request_body,
            Some(LinkRequestBody::Expression(RuntimeExpression::new(
                "$response.body#/id"
            )))
        );
    }

    #[test]
    fn test_extract_response_link_param_key_requires_name() {
        let raw = json!({
            "200": {
                "description": "ok",
                "links": {
                    "Self": {
                        "operationId": "getUser",
                        "parameters": {
                            "path.": "$request.path.id"
                        }
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let err = extract_response_details(&shim, None).expect_err("expected error");
        assert!(format!("{err}").contains("parameter key 'path.'"));
    }

    #[test]
    fn test_extract_response_link_missing_operation_is_error() {
        let link = LinkBuilder::new().description("Missing op").build();

        let response = ResponseBuilder::new()
            .description("Linked Response")
            .link("BadLink", link)
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let err = extract_response_details(&wrap(responses), None).expect_err("expected error");
        assert!(
            format!("{err}").contains("must define exactly one of 'operationId' or 'operationRef'")
        );
    }

    #[test]
    fn test_extract_response_link_with_both_operations_is_error() {
        let link = LinkBuilder::new()
            .operation_id("getUser")
            .operation_ref("#/paths/~1users~1{id}/get")
            .build();

        let response = ResponseBuilder::new()
            .description("Linked Response")
            .link("BadLink", link)
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let err = extract_response_details(&wrap(responses), None).expect_err("expected error");
        assert!(
            format!("{err}").contains("must define exactly one of 'operationId' or 'operationRef'")
        );
    }

    #[test]
    fn test_extract_vendor_json_media_type() {
        let response = ResponseBuilder::new()
            .description("Vendor JSON")
            .content(
                "application/vnd.api+json",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/User",
                )))),
            )
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.expect("Missing body type"), "User");
    }

    #[test]
    fn test_extract_response_media_type_ref() {
        let raw = json!({
            "200": {
                "description": "OK",
                "content": {
                    "application/json": {
                        "$ref": "#/components/mediaTypes/UserJson"
                    }
                }
            }
        });
        let responses = ShimResponses::from_raw(raw).expect("Failed to parse from raw");

        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            self_uri: None,
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "mediaTypes".to_string(),
            json!({
                "UserJson": {
                    "schema": { "$ref": "#/components/schemas/User" }
                }
            }),
        );

        let details = extract_response_details(&responses, Some(&components))
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.expect("Missing body type"), "User");
    }

    #[test]
    fn test_extract_wildcard_media_type() {
        let response = ResponseBuilder::new()
            .description("Wildcard")
            .content(
                "*/*",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/Anything",
                )))),
            )
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.expect("Missing body type"), "Anything");
    }

    #[test]
    fn test_extract_header_content_media_type_ref() {
        let raw = json!({
            "200": {
                "description": "OK",
                "headers": {
                    "X-Link": {
                        "content": {
                            "application/linkset": {
                                "$ref": "#/components/mediaTypes/LinkHeader"
                            }
                        }
                    }
                }
            }
        });
        let responses = ShimResponses::from_raw(raw).expect("Failed to parse from raw");

        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            self_uri: None,
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "mediaTypes".to_string(),
            json!({
                "LinkHeader": {
                    "schema": { "type": "string" }
                }
            }),
        );

        let details = extract_response_details(&responses, Some(&components))
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.headers.len(), 1);
        assert_eq!(details.headers[0].name, "X-Link");
        assert_eq!(details.headers[0].ty, "String");
    }

    #[test]
    fn test_extract_header_content_multiple_entries_rejected() {
        let raw = json!({
            "200": {
                "description": "OK",
                "headers": {
                    "X-Bad": {
                        "content": {
                            "text/plain": {},
                            "application/json": {}
                        }
                    }
                }
            }
        });
        let responses = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let err = extract_response_details(&responses, None).expect_err("expected error");
        assert!(format!("{err}").contains("must define exactly one media type"));
    }

    #[test]
    fn test_extract_text_plain_without_schema() {
        let response = ResponseBuilder::new()
            .description("Plain")
            .content("text/plain", Content::new::<RefOr<Schema>>(None))
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.expect("Missing body type"), "String");
    }

    #[test]
    fn test_extract_text_wildcard_without_schema() {
        let response = ResponseBuilder::new()
            .description("Text Wildcard")
            .content("text/*", Content::new::<RefOr<Schema>>(None))
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.expect("Missing body type"), "String");
    }

    #[test]
    fn test_extract_octet_stream_without_schema() {
        let response = ResponseBuilder::new()
            .description("Binary")
            .content(
                "application/octet-stream",
                Content::new::<RefOr<Schema>>(None),
            )
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.expect("Missing body type"), "Vec<u8>");
    }

    #[test]
    fn test_extract_json_without_schema_defaults_value() {
        let response = ResponseBuilder::new()
            .description("Json Any")
            .content("application/json", Content::new::<RefOr<Schema>>(None))
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(
            details.body_type.expect("Missing body type"),
            "serde_json::Value"
        );
    }

    #[test]
    fn test_ignore_content_type_header() {
        let response = ResponseBuilder::new()
            .description("Headers")
            .header(
                "Content-Type",
                HeaderBuilder::new()
                    .schema(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                        "#/components/schemas/Ignored",
                    ))))
                    .build(),
            )
            .header(
                "X-Rate-Limit",
                HeaderBuilder::new()
                    .schema(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                        "#/components/schemas/Limit",
                    ))))
                    .build(),
            )
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.headers.len(), 1);
        assert_eq!(details.headers[0].name, "X-Rate-Limit");
    }

    #[test]
    fn test_extract_response_header_content_schema_type() {
        let raw = json!({
            "200": {
                "description": "OK",
                "headers": {
                    "X-Rate-Limit": {
                        "content": {
                            "text/plain": {
                                "schema": { "type": "integer", "format": "int32" }
                            }
                        }
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&shim, None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.headers.len(), 1);
        assert_eq!(details.headers[0].name, "X-Rate-Limit");
        assert_eq!(details.headers[0].ty, "i32");
    }

    #[test]
    fn test_extract_response_header_content_media_type_and_example() {
        let raw = json!({
            "200": {
                "description": "OK",
                "headers": {
                    "X-Token": {
                        "content": {
                            "text/plain": {
                                "schema": { "type": "string" },
                                "examples": {
                                    "token": { "serializedValue": "abc123" }
                                }
                            }
                        }
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&shim, None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.headers.len(), 1);
        let header = &details.headers[0];
        assert_eq!(header.content_media_type.as_deref(), Some("text/plain"));
        assert!(header
            .example
            .as_ref()
            .map(|e| e.is_serialized())
            .unwrap_or(false));
    }

    #[test]
    fn test_extract_response_header_metadata() {
        let raw = json!({
            "200": {
                "description": "OK",
                "headers": {
                    "X-Flag": {
                        "required": true,
                        "deprecated": true,
                        "style": "simple",
                        "explode": true,
                        "schema": { "type": "string" }
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&shim, None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        let header = &details.headers[0];
        assert!(header.required);
        assert!(header.deprecated);
        assert_eq!(header.style, Some(ParamStyle::Simple));
        assert_eq!(header.explode, Some(true));
    }

    #[test]
    fn test_extract_response_header_invalid_style() {
        let raw = json!({
            "200": {
                "description": "OK",
                "headers": {
                    "X-Flag": {
                        "style": "form",
                        "schema": { "type": "string" }
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let err = extract_response_details(&shim, None).expect_err("expected error");
        assert!(format!("{err}").contains("style 'form'"));
    }

    #[test]
    fn test_extract_response_header_content_style_conflict() {
        let raw = json!({
            "200": {
                "description": "OK",
                "headers": {
                    "X-Flag": {
                        "content": {
                            "text/plain": {
                                "schema": { "type": "string" }
                            }
                        },
                        "style": "simple"
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let err = extract_response_details(&shim, None).expect_err("expected error");
        assert!(format!("{err}").contains("must not define style or explode"));
    }

    #[test]
    fn test_extract_response_header_ref_description_override() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            self_uri: None,
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "headers".to_string(),
            json!({
                "RateHeader": {
                    "schema": { "type": "integer" },
                    "description": "original"
                }
            }),
        );

        let raw = json!({
            "200": {
                "description": "OK",
                "headers": {
                    "X-Rate": {
                        "$ref": "#/components/headers/RateHeader",
                        "description": "override"
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&shim, Some(&components))
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.headers.len(), 1);
        let header = &details.headers[0];
        assert_eq!(header.name, "X-Rate");
        assert_eq!(header.ty, "i32");
        assert_eq!(header.description.as_deref(), Some("override"));
    }

    #[test]
    fn test_extract_response_header_content_multiple_media_types_error() {
        let raw = json!({
            "200": {
                "description": "OK",
                "headers": {
                    "X-Rate": {
                        "content": {
                            "text/plain": { "schema": { "type": "string" } },
                            "application/json": { "schema": { "type": "string" } }
                        }
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let err = extract_response_details(&shim, None).expect_err("expected error");
        assert!(format!("{err}").contains("content must define exactly one media type"));
    }

    #[test]
    fn test_extract_response_header_schema_true_maps_to_string() {
        let raw = json!({
            "200": {
                "description": "OK",
                "headers": {
                    "X-Any": {
                        "schema": true
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let details = extract_response_details(&shim, None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.headers.len(), 1);
        assert_eq!(details.headers[0].name, "X-Any");
        assert_eq!(details.headers[0].ty, "String");
    }

    #[test]
    fn test_extract_response_header_schema_false_rejected() {
        let raw = json!({
            "200": {
                "description": "OK",
                "headers": {
                    "X-Any": {
                        "schema": false
                    }
                }
            }
        });

        let shim = ShimResponses::from_raw(raw).expect("Failed to parse from raw");
        let err = extract_response_details(&shim, None).expect_err("expected error");
        assert!(format!("{err}").contains("schema is 'false'"));
    }

    #[test]
    fn test_extract_response_ref_with_self() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            self_uri: Some("https://example.com/openapi.yaml".to_string()),
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "responses".to_string(),
            json!({
                "OkResp": {
                    "description": "OK",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/User" }
                        }
                    }
                }
            }),
        );

        let mut responses = Responses::new();
        responses.responses.insert(
            "200".into(),
            RefOr::Ref(utoipa::openapi::Ref::new(
                "https://example.com/openapi.yaml#/components/responses/OkResp",
            )),
        );

        let details = extract_response_details(&wrap(responses), Some(&components))
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(details.body_type.as_deref(), Some("User"));
    }

    #[test]
    fn test_extract_response_link_literal_and_server() {
        let link = LinkBuilder::new()
            .operation_ref("/users/{id}")
            .parameter("id", 42)
            .server(Some(Server::new("https://api.example.com")))
            .request_body(Some(json!({"note": "hello"})))
            .build();

        let response = ResponseBuilder::new()
            .description("Linked Response")
            .link("UserLink", link)
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .expect("Failed to extract response details")
            .expect("Missing value");
        let link = &details.links[0];
        assert_eq!(link.server_url.as_deref(), Some("https://api.example.com"));
        assert!(matches!(
            link.request_body,
            Some(LinkRequestBody::Literal(_))
        ));
        assert_eq!(
            link.parameters.get("id"),
            Some(&LinkParamValue::Literal(json!(42)))
        );
    }

    #[test]
    fn test_extract_response_link_ref_description_override() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            self_uri: None,
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "links".to_string(),
            json!({
                "UserLink": {
                    "operationId": "getUser"
                }
            }),
        );

        let mut link_ref = utoipa::openapi::Ref::new("#/components/links/UserLink");
        link_ref.description = "Override description".to_string();

        let mut response = ResponseBuilder::new().description("Linked").build();
        response
            .links
            .insert("UserLink".to_string(), RefOr::Ref(link_ref));

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), Some(&components))
            .expect("Failed to extract response details")
            .expect("Missing value");
        assert_eq!(
            details.links[0].description.as_deref(),
            Some("Override description")
        );

        // Comprehensive tests for remaining branches in responses.rs:
        // 1. Concrete 2xx fallback (e.g. 202), no matching response (e.g. 404, 2a, 20a), unresolved Ref, Ref with summary/description, empty description
        let mut r_202 = Responses::new();
        r_202.responses.insert(
            "10".into(),
            RefOr::T(ResponseBuilder::new().description("short").build()),
        );
        r_202.responses.insert(
            "100".into(),
            RefOr::T(ResponseBuilder::new().description("info").build()),
        );
        r_202.responses.insert(
            "2a".into(),
            RefOr::T(ResponseBuilder::new().description("bad").build()),
        );
        r_202.responses.insert(
            "20a".into(),
            RefOr::T(ResponseBuilder::new().description("bad").build()),
        );
        r_202.responses.insert(
            "202".into(),
            RefOr::T(ResponseBuilder::new().description("   ").build()),
        );
        let d_202 = extract_response_details(&wrap(r_202), None)
            .expect("202")
            .expect("some");
        assert_eq!(d_202.status_code.as_deref(), Some("202"));
        assert!(d_202.description.is_none());

        let item_false_raw = json!({
            "200": {
                "description": "ok",
                "content": {
                    "application/json": { "itemSchema": false }
                }
            }
        });
        let item_false_shim = ShimResponses::from_raw(item_false_raw).expect("item_false");
        assert!(extract_response_details(&item_false_shim, None)
            .expect("ok")
            .expect("some")
            .body_type
            .is_none());

        let resp_typed_hdrs = ResponseBuilder::new()
            .description("ok")
            .header(
                "Content-Type",
                HeaderBuilder::new()
                    .schema(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                        "#/components/schemas/S",
                    ))))
                    .build(),
            )
            .header(
                "X-Typed",
                HeaderBuilder::new()
                    .schema(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                        "#/components/schemas/S",
                    ))))
                    .build(),
            )
            .build();
        let typed_hdrs =
            extract_response_headers(&resp_typed_hdrs, None, None, None, None).expect("typed_hdrs");
        assert_eq!(typed_hdrs.len(), 1);

        let mut r_404 = Responses::new();
        r_404.responses.insert(
            "404".into(),
            RefOr::T(ResponseBuilder::new().description("Not Found").build()),
        );
        assert!(extract_response_details(&wrap(r_404), None)
            .expect("404")
            .is_none());

        let mut r_unres = Responses::new();
        r_unres.responses.insert(
            "200".into(),
            RefOr::Ref(utoipa::openapi::Ref::new("#/components/responses/Missing")),
        );
        assert!(extract_response_details(&wrap(r_unres), Some(&components))
            .expect("unres")
            .is_none());

        // 2. External registry response, mediaTypes, headers, and links resolution
        let mut reg = DocumentRegistry::new();
        reg.register_openapi_yaml(
            "https://example.com/ext.yaml",
            r#"
openapi: 3.2.0
info: { title: Ext, version: "1.0" }
paths: {}
components:
  responses:
    ExtResp:
      description: ExtDesc
      content:
        application/json:
          $ref: '#/components/mediaTypes/ExtMedia'
  mediaTypes:
    ExtMedia:
      itemSchema:
        type: integer
  headers:
    ExtHdr:
      schema:
        type: string
  links:
    ExtLink:
      operationId: extOp
"#,
        )
        .expect("reg");
        let mut ref_ext =
            utoipa::openapi::Ref::new("https://example.com/ext.yaml#/components/responses/ExtResp");
        ref_ext.summary = "SumOvr".to_string();
        ref_ext.description = "DescOvr".to_string();
        let mut r_ext = Responses::new();
        r_ext.responses.insert("200".into(), RefOr::Ref(ref_ext));
        let d_ext = extract_response_details_with_registry(&wrap(r_ext), None, Some(&reg), None)
            .expect("ext")
            .expect("some");
        assert_eq!(d_ext.summary.as_deref(), Some("SumOvr"));
        assert_eq!(d_ext.description.as_deref(), Some("DescOvr"));
        assert_eq!(d_ext.body_type.as_deref(), Some("i32"));

        // 3. Server with variables on typed Link
        let srv = ServerBuilder::new()
            .url("https://{env}.example.com")
            .parameter(
                "env",
                ServerVariableBuilder::new()
                    .default_value("prod")
                    .enum_values(Some(["prod", "staging"]))
                    .description(Some("env"))
                    .build(),
            )
            .build();
        let link_with_vars = LinkBuilder::new()
            .operation_id("opWithSrv")
            .server(Some(srv))
            .build();
        let resp_srv = ResponseBuilder::new()
            .description("ok")
            .link("L", link_with_vars)
            .build();
        let mut r_srv = Responses::new();
        r_srv.responses.insert("200".into(), RefOr::T(resp_srv));
        let d_srv = extract_response_details(&wrap(r_srv), None)
            .expect("srv")
            .expect("some");
        assert_eq!(
            d_srv.links[0].server_url.as_deref(),
            Some("https://prod.example.com")
        );

        // 4. Headers: Content-Type ignored in raw, non-object header, no schema/content, content with itemSchema (sequential & non-sequential),
        // content without schema/itemSchema, schema example & examples map, media example & examples map, cycle in header ref, unresolved header ref, invalid schema
        let mut rich_comps = ShimComponents::default();
        rich_comps.extra.insert(
            "headers".to_string(),
            json!({
                "CycleH": { "$ref": "#/components/headers/CycleH" }
            }),
        );
        rich_comps.extra.insert(
            "mediaTypes".to_string(),
            json!({
                "CycleM": { "$ref": "#/components/mediaTypes/CycleM" }
            }),
        );
        rich_comps.extra.insert(
            "links".to_string(),
            json!({
                "CycleL": { "$ref": "#/components/links/CycleL" },
                "RawCustomLink": {
                    "operationRef": "/items/{id}",
                    "description": "CustomDesc",
                    "parameters": { "id": "$response.body#/id" },
                    "requestBody": "$response.body",
                    "server": {
                        "url": "https://{host}/v1",
                        "description": "SrvDesc",
                        "name": "SrvName",
                        "variables": {
                            "host": {
                                "default": "api.local",
                                "enum": ["api.local"],
                                "description": "HostVar"
                            },
                            "bad_var": "not_an_obj",
                            "no_default": {}
                        }
                    }
                }
            }),
        );

        let raw_headers_rich = json!({
            "Content-Type": { "schema": { "type": "string" } },
            "X-No-Schema": { "x-ext": 1 },
            "X-Schema-Ex": {
                "schema": { "type": "string" },
                "example": "h_ex"
            },
            "X-Schema-Examples": {
                "schema": { "type": "string" },
                "examples": {
                    "empty": {},
                    "valid": { "value": "h_exs" }
                }
            },
            "X-Schema-Examples-Empty": {
                "schema": { "type": "string" },
                "examples": {
                    "empty": {}
                }
            },
            "X-Content-Item-Seq": {
                "content": {
                    "application/jsonl": {
                        "itemSchema": { "type": "string" },
                        "example": "m_ex"
                    }
                }
            },
            "X-Content-Item-NonSeq": {
                "content": {
                    "application/json": {
                        "itemSchema": { "type": "string" },
                        "examples": {
                            "empty": {}
                        }
                    }
                }
            },
            "X-Content-No-Schema": {
                "content": {
                    "custom/unknown": {}
                }
            },
            "X-Ext-Ref": {
                "$ref": "https://example.com/ext.yaml#/components/headers/ExtHdr"
            }
        });
        let extracted_hdrs = extract_headers_from_raw(
            raw_headers_rich.as_object().expect("obj"),
            Some(&rich_comps),
            Some(&reg),
            None,
        )
        .expect("rich hdrs");
        assert!(!extracted_hdrs.is_empty());

        for bad_hdr_map in [
            json!({ "H": "not_an_obj" }),
            json!({ "H": { "content": "not_an_obj" } }),
            json!({ "H": { "content": { "text/plain": "not_an_obj" } } }),
            json!({ "H": { "content": { "text/plain": { "itemSchema": 123 } } } }),
            json!({ "H": { "schema": 123 } }),
            json!({ "H": { "$ref": "#/components/headers/CycleH" } }),
            json!({ "H": { "$ref": "#/components/headers/MissingH" } }),
        ] {
            assert!(extract_headers_from_raw(
                bad_hdr_map.as_object().expect("obj"),
                Some(&rich_comps),
                None,
                None
            )
            .is_err());
        }
        assert!(extract_header_example_from_media(&json!("not_obj"), None, "text/plain").is_none());

        // 5. Media types: raw_media_for_type cycle, external ref, missing ref, non-object; infer_body_type_from_media_type variants
        assert!(raw_media_for_type(
            &json!({ "content": { "application/json": { "$ref": "https://example.com/ext.yaml#/components/mediaTypes/ExtMedia" } } }),
            "application/json",
            None,
            Some(&reg),
            None
        )
        .is_some());
        assert!(raw_media_for_type(
            &json!({ "content": { "application/json": { "$ref": "#/components/mediaTypes/CycleM" } } }),
            "application/json",
            Some(&rich_comps),
            None,
            None
        )
        .is_some());
        assert!(raw_media_for_type(
            &json!({ "content": { "application/json": { "$ref": "#/components/mediaTypes/MissingM" } } }),
            "application/json",
            Some(&rich_comps),
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

        for (mt, expected) in [
            ("application/jsonl", Some("Vec<serde_json::Value>")),
            ("application/pdf", Some("Vec<u8>")),
            ("image/png", Some("Vec<u8>")),
            ("audio/mp3", Some("Vec<u8>")),
            ("video/mp4", Some("Vec<u8>")),
            ("application/*", None),
            ("*/*", None),
            ("custom/unknown", None),
        ] {
            let resp = ResponseBuilder::new()
                .description("ok")
                .content(mt, Content::new::<RefOr<Schema>>(None))
                .build();
            let mut rs = Responses::new();
            rs.responses.insert("200".into(), RefOr::T(resp));
            let d = extract_response_details(&wrap(rs), None)
                .expect("ok")
                .expect("some");
            assert_eq!(d.body_type.as_deref(), expected);
        }

        // 6. Links: link_from_raw, normalize_link_object non-object, extract_links_from_raw (non-object root, non-object link, cycle, missing ref,
        // both/neither operationId and operationRef, invalid parameters, server with variables, summary & description override on ref)
        let mut non_obj_link = json!("not_an_obj");
        normalize_link_object(&mut non_obj_link);
        let mut already_snake_link = json!({ "operation_id": "op1" });
        normalize_link_object(&mut already_snake_link);
        assert!(link_from_raw(&json!("not_an_obj")).is_none());
        let mut raw_custom = rich_comps.extra["links"]["RawCustomLink"].clone();
        normalize_link_object(&mut raw_custom);
        let raw_built = link_from_raw(&raw_custom).expect("raw link");
        assert_eq!(raw_built.operation_ref, "/items/{id}");

        let mut ext_link_ref =
            utoipa::openapi::Ref::new("https://example.com/ext.yaml#/components/links/ExtLink");
        assert!(resolve_link_from_ref(&ext_link_ref, None, Some(&reg), None).is_some());
        ext_link_ref.description = "ExtLinkDesc".to_string();
        assert!(resolve_link_from_ref(&ext_link_ref, None, Some(&reg), None).is_some());
        let mut resp_unres_link = ResponseBuilder::new().description("ok").build();
        resp_unres_link.links.insert(
            "MissingL".to_string(),
            RefOr::Ref(utoipa::openapi::Ref::new("#/components/links/MissingL")),
        );
        let mut rs_unres_link = Responses::new();
        rs_unres_link
            .responses
            .insert("200".into(), RefOr::T(resp_unres_link));
        let _ =
            extract_response_details(&wrap(rs_unres_link), Some(&rich_comps)).expect("unres link");

        assert!(
            extract_links_from_raw(&json!("not_an_obj"), None, None, None)
                .expect("empty")
                .is_empty()
        );

        let valid_raw_links = json!({
            "L1": {
                "$ref": "#/components/links/RawCustomLink",
                "description": "OvrDesc",
                "summary": "OvrSum"
            },
            "L2": {
                "$ref": "https://example.com/ext.yaml#/components/links/ExtLink"
            },
            "L3": {
                "operationId": "op3",
                "parameters": {
                    "literal_str": "hello",
                    "literal_num": 123,
                    "path.id": "ok",
                    "other.param": "ok"
                },
                "requestBody": "plain_str_body",
                "server": "not_a_server_obj"
            },
            "L4": {
                "operationId": "op4"
            }
        });
        let parsed_raw_links =
            extract_links_from_raw(&valid_raw_links, Some(&rich_comps), Some(&reg), None)
                .expect("valid_raw_links");
        assert_eq!(parsed_raw_links.len(), 4);
        assert_eq!(
            parsed_raw_links[0].server_url.as_deref(),
            Some("https://api.local/v1")
        );

        for bad_raw_links in [
            json!({ "L": "not_an_obj" }),
            json!({ "L": { "$ref": "#/components/links/CycleL" } }),
            json!({ "L": { "$ref": "#/components/links/MissingL" } }),
            json!({ "L": {} }),
            json!({ "L": { "operationId": "a", "operationRef": "/b" } }),
            json!({ "L": { "operationId": "a", "parameters": "not_an_obj" } }),
            json!({ "L": { "operationId": "a", "parameters": { "path.": "val" } } }),
            json!({ "L": { "operationId": "a", "parameters": { "p": "$invalid_expr" } } }),
            json!({ "L": { "operationId": "a", "requestBody": "$invalid_expr" } }),
        ] {
            assert!(extract_links_from_raw(&bad_raw_links, Some(&rich_comps), None, None).is_err());
        }
        assert!(resolve_response_from_components(
            &utoipa::openapi::Ref::new("#/components/responses/Missing"),
            Some(&ShimComponents::default()),
            None,
            None
        )
        .is_none());
        assert!(resolve_link_raw_from_components(
            "#/components/links/Missing",
            Some(&ShimComponents::default()),
            None,
            None
        )
        .is_none());
        assert!(resolve_header_from_components(
            "#/components/headers/Missing",
            Some(&ShimComponents::default()),
            None,
            None
        )
        .is_none());
        assert!(raw_media_for_type(
            &json!({ "content": { "application/json": { "$ref": "https://example.com/ext.yaml#/components/mediaTypes/ExtMedia" } } }),
            "application/json",
            Some(&ShimComponents::default()),
            Some(&reg),
            None
        )
        .is_some());

        // Cover closure false/None return paths for tarpaulin DWARF line tables
        let mut extra_comps = ShimComponents::default();
        extra_comps.extra.insert(
            "responses".to_string(),
            json!({ "BadResp": "not_a_response" }),
        );
        extra_comps.extra.insert(
            "mediaTypes".to_string(),
            json!({ "KnownMedia": { "schema": { "type": "string" } } }),
        );
        let mut extra_reg = DocumentRegistry::new();
        extra_reg
            .register_openapi_yaml(
                "https://example.com/extra.yaml",
                r#"
openapi: 3.2.0
info:
  title: Extra
  version: "1.0"
paths: {}
components:
  responses:
    BadExtResp: "not_a_response"
"#,
            )
            .expect("register extra");
        assert!(resolve_response_from_components(
            &utoipa::openapi::Ref::new("#/components/responses/BadResp"),
            Some(&extra_comps),
            Some(&extra_reg),
            None,
        )
        .is_none());
        assert!(resolve_response_from_components(
            &utoipa::openapi::Ref::new(
                "https://example.com/extra.yaml#/components/responses/BadExtResp"
            ),
            Some(&ShimComponents::default()),
            Some(&extra_reg),
            None,
        )
        .is_none());
        assert!(resolve_response_from_components(
            &utoipa::openapi::Ref::new("#/components/schemas/NotAResponse"),
            Some(&extra_comps),
            None,
            None,
        )
        .is_none());
        assert!(resolve_link_raw_from_components(
            "#/components/schemas/NotALink",
            Some(&extra_comps),
            Some(&extra_reg),
            None,
        )
        .is_none());
        assert!(resolve_header_from_components(
            "#/components/schemas/NotAHeader",
            Some(&extra_comps),
            Some(&extra_reg),
            None,
        )
        .is_none());
        assert!(raw_media_for_type(
            &json!({ "content": { "application/json": { "$ref": "#/components/schemas/NotMedia" } } }),
            "application/json",
            Some(&extra_comps),
            Some(&extra_reg),
            None,
        )
        .is_some());
        assert!(raw_media_for_type(
            &json!({ "content": { "application/json": { "$ref": "#/components/mediaTypes/MissingMedia" } } }),
            "application/json",
            Some(&extra_comps),
            Some(&extra_reg),
            None,
        )
        .is_some());

        // Cover Header { schema: None } and RefOr::Ref in extract_response_headers,
        // and fallback RefOr::Ref in select_response_media
        let mut resp_edge = utoipa::openapi::ResponseBuilder::new()
            .description("edge")
            .build();
        let mut hdr_no_schema = utoipa::openapi::header::HeaderBuilder::new().build();
        hdr_no_schema.schema = None;
        resp_edge
            .headers
            .insert("X-No-Schema".to_string(), RefOr::T(hdr_no_schema));
        resp_edge.headers.insert(
            "X-Ref-Hdr".to_string(),
            RefOr::Ref(utoipa::openapi::Ref::new("#/components/headers/RefHdr")),
        );
        let edge_hdrs =
            extract_response_headers(&resp_edge, None, None, None, None).expect("edge hdrs");
        assert_eq!(edge_hdrs.len(), 1);
        assert_eq!(edge_hdrs[0].name, "X-No-Schema");
        assert_eq!(edge_hdrs[0].ty, "String");

        let mut fallback_content = IndexMap::new();
        fallback_content.insert(
            "application/x-custom-unknown".to_string(),
            RefOr::Ref(utoipa::openapi::Ref::new(
                "#/components/mediaTypes/CustomRef",
            )),
        );
        let (sel_key, _) = select_response_media(&fallback_content).expect("fallback ref media");
        assert_eq!(sel_key, "application/x-custom-unknown");
    }
}
