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
    LinkParamValue, LinkRequestBody, ParsedLink, ResponseHeader, RuntimeExpression,
};
use crate::oas::ref_utils::extract_component_name;
use crate::oas::resolver::types::map_schema_to_rust_type;
use crate::oas::routes::shims::{ShimComponents, ShimResponses};
use indexmap::IndexMap;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::collections::HashSet;
use utoipa::openapi::{server::Server, Content, RefOr, Schema};

// Re-export specific structs if needed for external visibility
pub use crate::oas::models::ParsedLink as LinkModel;

/// The extracted details of a successful response.
#[derive(Debug)]
pub struct ParsedResponseDetails {
    /// The Rust type name of the body (if JSON).
    pub body_type: Option<String>,
    /// Extracted headers.
    pub headers: Vec<ResponseHeader>,
    /// Extracted links.
    pub links: Vec<ParsedLink>,
}

/// Extracts the success response type and its headers/links.
pub fn extract_response_details(
    responses: &ShimResponses,
    components: Option<&ShimComponents>,
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
        let (response, raw_response) = match resp_item {
            RefOr::T(val) => (
                Some(val.clone()),
                chosen_key.and_then(|key| responses.raw.get(key)).cloned(),
            ),
            RefOr::Ref(r) => {
                let mut resolved = resolve_response_from_components(r, components);
                if let Some(resp) = resolved.as_mut() {
                    if !r.description.is_empty() {
                        resp.description = r.description.clone();
                    }
                }
                let raw = resolve_response_raw_from_components(&r.ref_location, components);
                (resolved, raw)
            }
        };

        if let Some(r) = response {
            // 1. Resolve Body Type
            let body_type = select_response_media(&r.content)
                .map(|(media_type, media)| {
                    if let Some(schema) = media.schema.as_ref() {
                        return map_schema_to_rust_type(schema, true).map(Some);
                    }

                    let raw_media = raw_response
                        .as_ref()
                        .and_then(|raw| raw_media_for_type(raw, media_type, components));
                    if raw_schema_is_false(raw_media.as_ref(), "schema")
                        || raw_schema_is_false(raw_media.as_ref(), "itemSchema")
                    {
                        return Ok(None);
                    }
                    if let Some(schema_ref) = raw_media.as_ref().and_then(extract_media_schema) {
                        return map_schema_to_rust_type(&schema_ref, true).map(Some);
                    }
                    if let Some(item_schema) = raw_media.as_ref().and_then(extract_item_schema) {
                        let inner = map_schema_to_rust_type(&item_schema, true)?;
                        if is_sequential_media_type(&normalize_media_type(media_type)) {
                            return Ok(Some(format!("Vec<{}>", inner)));
                        }
                        return Ok(Some(inner));
                    }

                    Ok(infer_body_type_from_media_type(media_type))
                })
                .transpose()?
                .flatten();

            // 2. Resolve Headers
            let headers = extract_response_headers(&r, raw_response.as_ref(), components)?;

            // 3. Resolve Links
            let mut links = Vec::new();
            if !r.links.is_empty() {
                for (name, link_val) in &r.links {
                    let link_obj = match link_val {
                        RefOr::T(l) => Some(l.clone()),
                        RefOr::Ref(r) => resolve_link_from_ref(r, components),
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

                        let parameters = parse_link_parameters(&l.parameters)?;
                        let request_body = parse_link_request_body(&l.request_body)?;

                        links.push(ParsedLink {
                            name: name.clone(),
                            description,
                            operation_id,
                            operation_ref,
                            parameters,
                            request_body,
                            server_url: l.server.as_ref().map(resolve_server_url),
                        });
                    }
                }
            } else if let Some(raw_links) = raw_response.as_ref().and_then(|raw| raw.get("links")) {
                links = extract_links_from_raw(raw_links, components)?;
            }

            return Ok(Some(ParsedResponseDetails {
                body_type,
                headers,
                links,
            }));
        }
    }

    Ok(None)
}

fn resolve_server_url(server: &Server) -> String {
    let mut url = server.url.clone();
    if let Some(vars) = &server.variables {
        for (name, var) in vars {
            let placeholder = format!("{{{}}}", name);
            url = url.replace(&placeholder, &var.default_value);
        }
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

fn extract_response_headers(
    response: &utoipa::openapi::Response,
    raw_response: Option<&JsonValue>,
    components: Option<&ShimComponents>,
) -> AppResult<Vec<ResponseHeader>> {
    if let Some(raw_headers) = raw_response
        .and_then(|raw| raw.get("headers"))
        .and_then(|headers| headers.as_object())
    {
        return extract_headers_from_raw(raw_headers, components);
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
            ty,
        });
    }
    Ok(headers)
}

/// Extracts header definitions from raw response JSON so OAS 3.2 `content` is honored.
fn extract_headers_from_raw(
    raw_headers: &serde_json::Map<String, JsonValue>,
    components: Option<&ShimComponents>,
) -> AppResult<Vec<ResponseHeader>> {
    let mut headers = Vec::new();
    for (name, raw_header) in raw_headers {
        if name.eq_ignore_ascii_case("content-type") {
            continue;
        }

        let mut visited = HashSet::new();
        let resolved = resolve_header_value(raw_header, components, &mut visited)?;
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

        let ty = if let Some(schema_val) = obj.get("schema") {
            let schema: RefOr<Schema> =
                serde_json::from_value(schema_val.clone()).map_err(|e| {
                    AppError::General(format!(
                        "Failed to parse header schema for '{}': {}",
                        name, e
                    ))
                })?;
            map_schema_to_rust_type(&schema, true)?
        } else if let Some(content_val) = obj.get("content") {
            extract_header_type_from_content(name, content_val, components)?
        } else {
            "String".to_string()
        };

        headers.push(ResponseHeader {
            name: name.clone(),
            description,
            ty,
        });
    }

    Ok(headers)
}

/// Resolves a header type when using `content`.
fn extract_header_type_from_content(
    name: &str,
    content_val: &JsonValue,
    components: Option<&ShimComponents>,
) -> AppResult<String> {
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
    let resolved = resolve_media_type_ref(media_obj, components, &mut HashSet::new())
        .unwrap_or_else(|| media_obj.clone());
    let Some(resolved_obj) = resolved.as_object() else {
        return Err(AppError::General(format!(
            "Header '{}' content must be an object",
            name
        )));
    };

    if let Some(schema_val) = resolved_obj.get("schema") {
        let schema: RefOr<Schema> = serde_json::from_value(schema_val.clone()).map_err(|e| {
            AppError::General(format!(
                "Failed to parse header schema for '{}': {}",
                name, e
            ))
        })?;
        return map_schema_to_rust_type(&schema, true);
    }

    Ok(infer_body_type_from_media_type(media_type).unwrap_or_else(|| "String".to_string()))
}

/// Resolves a header that may be a `$ref`, applying description overrides.
fn resolve_header_value(
    raw_header: &JsonValue,
    components: Option<&ShimComponents>,
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

    let mut resolved = resolve_header_from_components(ref_str, components).ok_or_else(|| {
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

/// Resolves a header component reference to its raw JSON definition.
fn resolve_header_from_components(
    ref_str: &str,
    components: Option<&ShimComponents>,
) -> Option<JsonValue> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let name = extract_component_name(ref_str, self_uri, "headers")?;
    comps
        .extra
        .get("headers")
        .and_then(|h| h.get(&name))
        .cloned()
}

fn raw_media_for_type(
    raw_response: &serde_json::Value,
    media_type: &str,
    components: Option<&ShimComponents>,
) -> Option<serde_json::Value> {
    let media = raw_response
        .get("content")
        .and_then(|content| content.as_object())
        .and_then(|content| content.get(media_type))?;
    resolve_media_type_ref(media, components, &mut HashSet::new()).or_else(|| {
        if media.as_object().is_some() {
            Some(media.clone())
        } else {
            None
        }
    })
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
    if let Some(media) = content.get("application/json") {
        return Some(("application/json", media));
    }

    if let Some((key, media)) = content
        .iter()
        .find(|(k, _)| k.ends_with("+json") || k.as_str() == "application/*+json")
    {
        return Some((key.as_str(), media));
    }

    if let Some(media) = content.get("text/plain") {
        return Some(("text/plain", media));
    }

    if let Some((key, media)) = content.iter().find(|(k, _)| k.starts_with("text/")) {
        return Some((key.as_str(), media));
    }

    if let Some(media) = content.get("application/*") {
        return Some(("application/*", media));
    }

    if let Some(media) = content.get("*/*") {
        return Some(("*/*", media));
    }

    content.iter().next().map(|(k, media)| (k.as_str(), media))
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
) -> Option<utoipa::openapi::Response> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let ref_name = extract_component_name(&r.ref_location, self_uri, "responses")?;
    if let Some(resp_json) = comps.extra.get("responses").and_then(|r| r.get(&ref_name)) {
        if let Ok(resp) = serde_json::from_value::<utoipa::openapi::Response>(resp_json.clone()) {
            return Some(resp);
        }
    }
    None
}

fn resolve_response_raw_from_components(
    ref_str: &str,
    components: Option<&ShimComponents>,
) -> Option<serde_json::Value> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let ref_name = extract_component_name(ref_str, self_uri, "responses")?;
    comps
        .extra
        .get("responses")
        .and_then(|r| r.get(&ref_name))
        .cloned()
}

fn resolve_link_from_ref(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
) -> Option<utoipa::openapi::link::Link> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let ref_name = extract_component_name(&r.ref_location, self_uri, "links")?;
    if let Some(link_json) = comps.extra.get("links").and_then(|l| l.get(&ref_name)) {
        if let Ok(link) = serde_json::from_value::<utoipa::openapi::link::Link>(link_json.clone()) {
            return Some(link);
        }
    }
    None
}

fn resolve_link_raw_from_components(
    ref_str: &str,
    components: Option<&ShimComponents>,
) -> Option<JsonValue> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let name = extract_component_name(ref_str, self_uri, "links")?;
    comps.extra.get("links").and_then(|l| l.get(&name)).cloned()
}

fn resolve_link_value(
    raw_link: &JsonValue,
    components: Option<&ShimComponents>,
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

    let mut resolved = resolve_link_raw_from_components(ref_str, components).ok_or_else(|| {
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

fn resolve_server_url_from_raw(server: &JsonValue) -> Option<String> {
    let url = server.get("url")?.as_str()?.to_string();
    let mut resolved = url.clone();
    if let Some(vars) = server.get("variables").and_then(|v| v.as_object()) {
        for (name, var) in vars {
            if let Some(default) = var.get("default").and_then(|v| v.as_str()) {
                let placeholder = format!("{{{}}}", name);
                resolved = resolved.replace(&placeholder, default);
            }
        }
    }
    Some(resolved)
}

fn parse_link_parameters(
    parameters: &BTreeMap<String, serde_json::Value>,
) -> AppResult<std::collections::HashMap<String, LinkParamValue>> {
    let mut parsed = std::collections::HashMap::new();

    for (key, value) in parameters {
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
) -> AppResult<Vec<ParsedLink>> {
    let Some(map) = raw_links.as_object() else {
        return Ok(Vec::new());
    };

    let mut links = Vec::new();
    for (name, link_val) in map {
        let mut visited = HashSet::new();
        let resolved = resolve_link_value(link_val, components, &mut visited)?;
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
            parse_link_parameters(&params)?
        } else {
            std::collections::HashMap::new()
        };

        let request_body = parse_link_request_body(
            &obj.get("requestBody")
                .or_else(|| obj.get("request_body"))
                .cloned(),
        )?;
        let server_url = obj
            .get("server")
            .and_then(|server| resolve_server_url_from_raw(server));

        links.push(ParsedLink {
            name: name.clone(),
            description,
            operation_id,
            operation_ref,
            parameters,
            request_body,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::routes::shims::ShimComponents;
    use crate::oas::routes::shims::ShimResponses;
    use serde_json::json;
    use std::collections::BTreeMap;
    use utoipa::openapi::{
        header::HeaderBuilder, link::LinkBuilder, server::ServerBuilder,
        server::ServerVariableBuilder, Content, ResponseBuilder, Responses, Schema, Server,
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
}
