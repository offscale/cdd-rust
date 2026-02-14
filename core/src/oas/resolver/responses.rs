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
use crate::oas::models::{
    ExampleValue, LinkParamValue, LinkRequestBody, ParamStyle, ParsedLink, ParsedServer,
    ParsedServerVariable, ResponseHeader, RuntimeExpression,
};
use crate::oas::ref_utils::extract_component_name;
use crate::oas::registry::DocumentRegistry;
use crate::oas::resolver::types::{map_schema_to_rust_type, map_schema_to_rust_type_with_raw};
use crate::oas::routes::shims::{ShimComponents, ShimResponses};
use indexmap::IndexMap;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::collections::HashSet;
use url::Url;
use utoipa::openapi::{link::LinkBuilder, server::Server, Content, RefOr, Schema};

// Re-export specific structs if needed for external visibility
pub use crate::oas::models::ParsedLink as LinkModel;

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
            if key.starts_with('2') && key.len() == 3 && key.chars().all(char::is_numeric) {
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

                example = crate::oas::resolver::body::extract_media_example(
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

            let summary = summary_override.or_else(|| {
                raw_response
                    .as_ref()
                    .and_then(|raw| raw.get("summary"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });

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
    let variables = server
        .variables
        .as_ref()
        .map(|vars| {
            vars.iter()
                .map(|(name, var)| {
                    (
                        name.clone(),
                        ParsedServerVariable {
                            enum_values: var.enum_values.clone(),
                            default: var.default_value.clone(),
                            description: var.description.clone(),
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default();

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
        .filter(|(key, _)| key.starts_with("x-") && *key != "x-cdd-content")
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
    for (name, header_obj) in &response.headers {
        if name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        let ty = map_schema_to_rust_type(&header_obj.schema, true)?;
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

        let has_content = obj
            .get("content")
            .or_else(|| obj.get("x-cdd-content"))
            .is_some();
        let style = parse_header_style(name, obj.get("style").and_then(|v| v.as_str()))?;
        let explode = obj.get("explode").and_then(|v| v.as_bool());
        if has_content && (style.is_some() || explode.is_some()) {
            return Err(AppError::General(format!(
                "Header '{}' uses 'content' and must not define style or explode",
                name
            )));
        }

        let ty = if let Some(content_val) = obj.get("content").or_else(|| obj.get("x-cdd-content"))
        {
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

    let (media_type, media_obj) = content_map.iter().next().unwrap();
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
        if let Some(example) = crate::oas::resolver::body::extract_example_value(
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
        if let Some(example) = crate::oas::resolver::body::extract_example_value(
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

    if let Some(desc) = obj.get("description").and_then(|v| v.as_str()) {
        if let Some(resolved_obj) = resolved.as_object_mut() {
            resolved_obj.insert(
                "description".to_string(),
                JsonValue::String(desc.to_string()),
            );
        }
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
    if let Some((comps, self_uri)) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))
    {
        if let Some(name) = extract_component_name(ref_str, self_uri, "headers") {
            if let Some(header) = comps
                .extra
                .get("headers")
                .and_then(|h| h.get(&name))
                .cloned()
            {
                return Some(header);
            }
        }
    }

    registry
        .and_then(|registry| {
            registry.resolve_component_ref_with_components(ref_str, base_uri, "headers")
        })
        .map(|(raw, _, _)| raw)
}

fn raw_media_for_type(
    raw_response: &serde_json::Value,
    media_type: &str,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<serde_json::Value> {
    let media = raw_response
        .get("content")
        .and_then(|content| content.as_object())
        .and_then(|content| content.get(media_type))?;
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

/// Selects the most appropriate response content for JSON-like payloads.
///
/// Preference order:
/// 1. `application/json`
/// 2. Any `+json` media type (e.g. `application/vnd.api+json`)
/// 3. `application/*`
/// 4. `*/*`
/// 5. First available entry
fn select_response_media<'a>(
    content: &'a IndexMap<String, Content>,
) -> Option<(&'a str, &'a Content)> {
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

    content.iter().next().map(|(k, media)| (k.as_str(), media))
}

fn select_best_media<'a, F>(
    content: &'a IndexMap<String, Content>,
    predicate: F,
    preferred: Option<&str>,
) -> Option<(&'a str, &'a Content)>
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

fn infer_body_type_from_media_type(media_type: &str) -> Option<String> {
    let media = normalize_media_type(media_type);

    if is_sequential_json_media_type(&media) {
        return Some("Vec<serde_json::Value>".to_string());
    }

    if media == "application/json" || media.ends_with("+json") || media == "application/*+json" {
        return Some("serde_json::Value".to_string());
    }

    if media.starts_with("text/") || is_xml_media_type(&media) {
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
    media_type
        .split(';')
        .next()
        .unwrap_or(media_type)
        .trim()
        .to_ascii_lowercase()
}

fn is_json_media_type(media_type: &str) -> bool {
    let normalized = normalize_media_type(media_type);
    normalized == "application/json"
        || normalized.ends_with("+json")
        || normalized == "application/*+json"
        || is_sequential_json_media_type(&normalized)
}

fn is_text_media_type(media_type: &str) -> bool {
    let normalized = normalize_media_type(media_type);
    normalized.starts_with("text/") || is_xml_media_type(&normalized)
}

fn is_sequential_json_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "application/jsonl"
            | "application/x-ndjson"
            | "application/json-seq"
            | "application/geo+json-seq"
    ) || media_type.ends_with("+jsonl")
        || media_type.ends_with("+ndjson")
        || media_type.ends_with("+json-seq")
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

fn is_xml_media_type(media_type: &str) -> bool {
    media_type == "application/xml" || media_type == "text/xml" || media_type.ends_with("+xml")
}

fn resolve_response_from_components(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<ResolvedResponse> {
    if let Some((comps, self_uri)) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))
    {
        if let Some(ref_name) = extract_component_name(&r.ref_location, self_uri, "responses") {
            if let Some(resp_json) = comps.extra.get("responses").and_then(|r| r.get(&ref_name)) {
                if let Ok(resp) =
                    serde_json::from_value::<utoipa::openapi::Response>(resp_json.clone())
                {
                    return Some(ResolvedResponse {
                        response: resp,
                        raw: Some(resp_json.clone()),
                        components: None,
                        base_uri: None,
                    });
                }
            }
        }
    }

    if let Some(registry) = registry {
        if let Some((raw, comps_override, base_override)) =
            registry.resolve_component_ref_with_components(&r.ref_location, base_uri, "responses")
        {
            if let Ok(resp) = serde_json::from_value::<utoipa::openapi::Response>(raw.clone()) {
                return Some(ResolvedResponse {
                    response: resp,
                    raw: Some(raw),
                    components: comps_override,
                    base_uri: base_override,
                });
            }
        }
    }

    None
}

fn resolve_link_from_ref(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<utoipa::openapi::link::Link> {
    if let Some((comps, self_uri)) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))
    {
        if let Some(ref_name) = extract_component_name(&r.ref_location, self_uri, "links") {
            if let Some(link_json) = comps.extra.get("links").and_then(|l| l.get(&ref_name)) {
                return link_from_component_json(link_json, &r.description);
            }
        }
    }

    if let Some(registry) = registry {
        if let Some((raw, _, _)) =
            registry.resolve_component_ref_with_components(&r.ref_location, base_uri, "links")
        {
            return link_from_component_json(&raw, &r.description);
        }
    }

    None
}

fn normalize_link_object(value: &mut JsonValue) {
    let JsonValue::Object(link_obj) = value else {
        return;
    };

    if !link_obj.contains_key("operation_id") {
        if let Some(op_id) = link_obj.remove("operationId") {
            link_obj.insert("operation_id".to_string(), op_id);
        }
    }
    if !link_obj.contains_key("operation_ref") {
        if let Some(op_ref) = link_obj.remove("operationRef") {
            link_obj.insert("operation_ref".to_string(), op_ref);
        }
    }
    if !link_obj.contains_key("request_body") {
        if let Some(body) = link_obj.remove("requestBody") {
            link_obj.insert("request_body".to_string(), body);
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
        .or_else(|| link_from_raw(&normalized))?;
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
    if let Some((comps, self_uri)) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))
    {
        if let Some(name) = extract_component_name(ref_str, self_uri, "links") {
            if let Some(link) = comps.extra.get("links").and_then(|l| l.get(&name)) {
                return Some(link.clone());
            }
        }
    }

    registry
        .and_then(|registry| {
            registry.resolve_component_ref_with_components(ref_str, base_uri, "links")
        })
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

    if let Some(desc) = obj.get("description") {
        if let Some(resolved_obj) = resolved.as_object_mut() {
            resolved_obj.insert("description".to_string(), desc.clone());
        }
    }
    if let Some(summary) = obj.get("summary") {
        if let Some(resolved_obj) = resolved.as_object_mut() {
            resolved_obj.insert("summary".to_string(), summary.clone());
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
    if let Some(vars) = obj.get("variables").and_then(|v| v.as_object()) {
        for (var_name, var) in vars {
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
    use crate::oas::routes::shims::ShimComponents;
    use crate::oas::routes::shims::ShimResponses;
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
            .unwrap()
            .unwrap();
        assert_eq!(details.body_type.unwrap(), "User");
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
            .unwrap()
            .unwrap();
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
        let responses = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&responses, None).unwrap().unwrap();
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
        let responses = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&responses, None).unwrap().unwrap();
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
            .unwrap()
            .unwrap();
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
            .unwrap()
            .unwrap();
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
            .unwrap()
            .unwrap();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&shim, None).unwrap().unwrap();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&shim, None).unwrap().unwrap();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&shim, None).unwrap().unwrap();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&shim, None).unwrap().unwrap();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&shim, None).unwrap().unwrap();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&shim, None).unwrap().unwrap();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&shim, None).unwrap().unwrap();
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
            .unwrap()
            .unwrap();
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
            .unwrap()
            .unwrap();
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
            .unwrap()
            .unwrap();
        let l = &details.links[0];
        let expr = l.parameters.get("userId").unwrap();

        match expr {
            LinkParamValue::Expression(expr) => {
                assert_eq!(expr.as_str(), "$request.path.id");
                assert!(expr.is_expression());
            }
            _ => panic!("Expected runtime expression"),
        }
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
            .unwrap()
            .unwrap();
        let l = &details.links[0];
        let expr = l.parameters.get("userId").unwrap();

        match expr {
            LinkParamValue::Expression(expr) => {
                assert!(expr.as_str().contains("$request.path.id"));
                assert!(expr.is_expression());
            }
            _ => panic!("Expected runtime expression template"),
        }
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&shim, None).unwrap().unwrap();
        let link = &details.links[0];
        assert_eq!(link.operation_id.as_deref(), Some("getUser"));
        match link.request_body.as_ref() {
            Some(LinkRequestBody::Expression(expr)) => {
                assert_eq!(expr.as_str(), "$response.body#/id");
            }
            _ => panic!("expected runtime expression request body"),
        }
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let err = extract_response_details(&shim, None).unwrap_err();
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

        let err = extract_response_details(&wrap(responses), None).unwrap_err();
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

        let err = extract_response_details(&wrap(responses), None).unwrap_err();
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
            .unwrap()
            .unwrap();
        assert_eq!(details.body_type.unwrap(), "User");
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
        let responses = ShimResponses::from_raw(raw).unwrap();

        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
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
            .unwrap()
            .unwrap();
        assert_eq!(details.body_type.unwrap(), "User");
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
            .unwrap()
            .unwrap();
        assert_eq!(details.body_type.unwrap(), "Anything");
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
        let responses = ShimResponses::from_raw(raw).unwrap();

        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
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
            .unwrap()
            .unwrap();
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
        let responses = ShimResponses::from_raw(raw).unwrap();
        let err = extract_response_details(&responses, None).unwrap_err();
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
            .unwrap()
            .unwrap();
        assert_eq!(details.body_type.unwrap(), "String");
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
            .unwrap()
            .unwrap();
        assert_eq!(details.body_type.unwrap(), "String");
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
            .unwrap()
            .unwrap();
        assert_eq!(details.body_type.unwrap(), "Vec<u8>");
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
            .unwrap()
            .unwrap();
        assert_eq!(details.body_type.unwrap(), "serde_json::Value");
    }

    #[test]
    fn test_ignore_content_type_header() {
        let response = ResponseBuilder::new()
            .description("Headers")
            .header(
                "Content-Type",
                HeaderBuilder::new()
                    .schema(RefOr::Ref(utoipa::openapi::Ref::new(
                        "#/components/schemas/Ignored",
                    )))
                    .build(),
            )
            .header(
                "X-Rate-Limit",
                HeaderBuilder::new()
                    .schema(RefOr::Ref(utoipa::openapi::Ref::new(
                        "#/components/schemas/Limit",
                    )))
                    .build(),
            )
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&wrap(responses), None)
            .unwrap()
            .unwrap();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&shim, None).unwrap().unwrap();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&shim, None).unwrap().unwrap();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&shim, None).unwrap().unwrap();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let err = extract_response_details(&shim, None).unwrap_err();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let err = extract_response_details(&shim, None).unwrap_err();
        assert!(format!("{err}").contains("must not define style or explode"));
    }

    #[test]
    fn test_extract_response_header_ref_description_override() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&shim, Some(&components))
            .unwrap()
            .unwrap();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let err = extract_response_details(&shim, None).unwrap_err();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let details = extract_response_details(&shim, None).unwrap().unwrap();
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

        let shim = ShimResponses::from_raw(raw).unwrap();
        let err = extract_response_details(&shim, None).unwrap_err();
        assert!(format!("{err}").contains("schema is 'false'"));
    }

    #[test]
    fn test_extract_response_ref_with_self() {
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
            .unwrap()
            .unwrap();
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
            .unwrap()
            .unwrap();
        let link = &details.links[0];
        assert_eq!(link.server_url.as_deref(), Some("https://api.example.com"));
        assert!(matches!(
            link.request_body,
            Some(LinkRequestBody::Literal(_))
        ));
        match link.parameters.get("id").unwrap() {
            LinkParamValue::Literal(v) => assert_eq!(v, &json!(42)),
            _ => panic!("Expected literal parameter"),
        }
    }

    #[test]
    fn test_extract_response_link_ref_description_override() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
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
            .unwrap()
            .unwrap();
        assert_eq!(
            details.links[0].description.as_deref(),
            Some("Override description")
        );
    }
}
