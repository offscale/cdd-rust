#![deny(missing_docs)]

//! # Runtime Test Validation Support
//!
//! Reusable OpenAPI response, media-type, header, sequential JSON, and SSE validation
//! utilities used by generated contract tests and compiled/unit-tested directly in `cdd-core`.

use serde_json::{Map, Value};

/// Normalizes a media-type string by stripping parameters (`;charset=...`) and lowercasing.
///
/// # Arguments
/// * `media_type` - Raw media type string.
///
/// # Returns
/// Normalized media type string.
pub fn normalize_media_type(media_type: &str) -> String {
    crate::openapi::parse::models::MediaType::normalize(media_type)
}

/// Checks if a media type represents a sequential JSON stream (`application/jsonl`, `application/x-ndjson`, etc.).
///
/// # Arguments
/// * `media_type` - Media type string to check.
///
/// # Returns
/// `true` if `media_type` is sequential JSON.
pub fn is_sequential_json_media_type(media_type: &str) -> bool {
    crate::openapi::parse::models::MediaType::parse(media_type).is_sequential_json()
}

/// Checks if a media type represents JSON or a `+json` structured syntax suffix.
///
/// # Arguments
/// * `media_type` - Media type string to check.
///
/// # Returns
/// `true` if `media_type` is JSON-compatible.
pub fn is_json_media_type(media_type: &str) -> bool {
    crate::openapi::parse::models::MediaType::parse(media_type).is_json()
}

/// Checks if a media type represents plain text or XML.
///
/// # Arguments
/// * `media_type` - Media type string to check.
///
/// # Returns
/// `true` if `media_type` is text or XML.
pub fn is_text_media_type(media_type: &str) -> bool {
    crate::openapi::parse::models::MediaType::parse(media_type).is_text()
}

/// Checks if a media type represents Server-Sent Events (`text/event-stream`).
///
/// # Arguments
/// * `media_type` - Media type string to check.
///
/// # Returns
/// `true` if `media_type` is `text/event-stream`.
pub fn is_event_stream_media_type(media_type: &str) -> bool {
    crate::openapi::parse::models::MediaType::parse(media_type).is_event_stream()
}

/// Computes the specificity score of `pattern` matching `actual` (`3` = exact, `2` = prefix+suffix wildcard,
/// `1` = single-side wildcard, `0` = `*/*`).
///
/// # Arguments
/// * `pattern` - Documented media type pattern.
/// * `actual` - Actual `Content-Type` header value.
///
/// # Returns
/// `Some(score)` if `pattern` matches `actual`, or `None`.
pub fn media_type_specificity(pattern: &str, actual: &str) -> Option<i32> {
    let pattern = normalize_media_type(pattern);
    let actual = normalize_media_type(actual);

    if pattern == actual {
        return Some(3);
    }
    if pattern == "*/*" {
        return Some(0);
    }
    if let Some(idx) = pattern.find('*') {
        let (prefix, rest) = pattern.split_at(idx);
        let suffix = &rest[1..];
        if !prefix.is_empty() && !actual.starts_with(prefix) {
            return None;
        }
        if !suffix.is_empty() && !actual.ends_with(suffix) {
            return None;
        }
        let score = if !prefix.is_empty() && !suffix.is_empty() {
            2
        } else {
            1
        };
        return Some(score);
    }
    None
}

/// Selects the most specific media-type entry in `content` matching `content_type`.
///
/// # Arguments
/// * `content` - OpenAPI response `content` map.
/// * `content_type` - Actual response `Content-Type`.
///
/// # Returns
/// The matched `(media_type_key, media_type_object)` pair if found.
pub fn select_media_type_for_response<'a>(
    content: &'a Map<String, Value>,
    content_type: &str,
) -> Option<(String, &'a Value)> {
    let mut best: Option<(String, &'a Value, i32)> = None;
    for (key, value) in content {
        if let Some(score) = media_type_specificity(key, content_type) {
            let replace = match best.as_ref() {
                Some((_, _, best_score)) => score > *best_score,
                None => true,
            };
            if replace {
                best = Some((key.clone(), value, score));
            }
        }
    }
    best.map(|(k, v, _)| (k, v))
}

/// Selects a fallback media-type entry from `content` when no `Content-Type` header is present.
///
/// # Arguments
/// * `content` - OpenAPI response `content` map.
///
/// # Returns
/// The selected `(media_type_key, media_type_object)` pair if `content` is non-empty.
pub fn select_media_type(content: &Map<String, Value>) -> Option<(String, &Value)> {
    if let Some(media) = content.get("application/json") {
        return Some(("application/json".to_string(), media));
    }

    if let Some((key, media)) = content.iter().find(|(k, _)| {
        let normalized = normalize_media_type(k);
        normalized.ends_with("+json") || is_sequential_json_media_type(&normalized)
    }) {
        return Some((key.clone(), media));
    }

    if let Some(media) = content.get("text/plain") {
        return Some(("text/plain".to_string(), media));
    }

    if let Some((key, media)) = content.iter().find(|(k, _)| is_text_media_type(k)) {
        return Some((key.clone(), media));
    }

    if let Some(media) = content.get("application/*") {
        return Some(("application/*".to_string(), media));
    }

    if let Some(media) = content.get("*/*") {
        return Some(("*/*".to_string(), media));
    }

    content.iter().next().map(|(k, v)| (k.clone(), v))
}

/// Selects the documented response definition matching `status` (exact code, `NXX` range, or `default`).
///
/// # Arguments
/// * `responses` - OpenAPI `responses` object.
/// * `status` - HTTP status code.
///
/// # Returns
/// The matched response definition [`Value`] if found.
pub fn select_response_for_status(responses: &Value, status: u16) -> Option<&Value> {
    let map = responses.as_object()?;
    let status_key = status.to_string();
    if let Some(resp) = map.get(&status_key) {
        return Some(resp);
    }
    let range_key = format!("{}XX", status / 100);
    if let Some(resp) = map.get(&range_key) {
        return Some(resp);
    }
    let range_key_lower = format!("{}xx", status / 100);
    if let Some(resp) = map.get(&range_key_lower) {
        return Some(resp);
    }
    map.get("default")
}

/// Resolves a `$ref` on a response object against `components.responses` or `responses`.
///
/// # Arguments
/// * `resp_def` - Response object or reference.
/// * `openapi` - Root OpenAPI document.
///
/// # Returns
/// Resolved response object with any local `summary`/`description` overrides applied.
pub fn resolve_response_ref(resp_def: &Value, openapi: &Value) -> Value {
    let Some(ref_str) = resp_def.get("$ref").and_then(Value::as_str) else {
        return resp_def.clone();
    };

    let resolved = if let Some(name) = ref_str.strip_prefix("#/components/responses/") {
        openapi
            .get("components")
            .and_then(|c| c.get("responses"))
            .and_then(|r| r.get(name))
    } else if let Some(name) = ref_str.strip_prefix("#/responses/") {
        openapi.get("responses").and_then(|r| r.get(name))
    } else {
        None
    };

    let Some(resolved) = resolved else {
        return resp_def.clone();
    };

    let mut merged = resolved.clone();
    if let Value::Object(map) = &mut merged {
        if let Some(summary) = resp_def.get("summary") {
            map.insert("summary".to_string(), summary.clone());
        }
        if let Some(description) = resp_def.get("description") {
            map.insert("description".to_string(), description.clone());
        }
    }
    merged
}

/// Resolves a `$ref` on a header object against `components.headers`.
///
/// # Arguments
/// * `header_def` - Header object or reference.
/// * `openapi` - Root OpenAPI document.
///
/// # Returns
/// Resolved header object with any local `summary`/`description` overrides applied.
pub fn resolve_header_ref(header_def: &Value, openapi: &Value) -> Value {
    let Some(ref_str) = header_def.get("$ref").and_then(Value::as_str) else {
        return header_def.clone();
    };

    let resolved = if let Some(name) = ref_str.strip_prefix("#/components/headers/") {
        openapi
            .get("components")
            .and_then(|c| c.get("headers"))
            .and_then(|h| h.get(name))
    } else {
        None
    };

    let Some(resolved) = resolved else {
        return header_def.clone();
    };

    let mut merged = resolved.clone();
    if let Value::Object(map) = &mut merged {
        if let Some(summary) = header_def.get("summary") {
            map.insert("summary".to_string(), summary.clone());
        }
        if let Some(description) = header_def.get("description") {
            map.insert("description".to_string(), description.clone());
        }
    }
    merged
}

/// Parses a newline-delimited or RS-delimited (`\u{1e}`) sequential JSON body into a JSON array.
///
/// # Arguments
/// * `body` - Raw response body text.
///
/// # Returns
/// `Value::Array` containing each parsed JSON record.
pub fn parse_sequential_json(body: &str) -> Value {
    let mut items = Vec::new();
    let chunks: Vec<&str> = if body.contains('\u{1e}') {
        body.split('\u{1e}').collect()
    } else {
        body.lines().collect()
    };

    for chunk in chunks {
        let trimmed = chunk.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
            items.push(val);
        }
    }

    Value::Array(items)
}

/// Parses a `text/event-stream` (SSE) payload into a JSON array of event objects.
///
/// # Arguments
/// * `body` - Raw SSE stream text.
///
/// # Returns
/// `Value::Array` of structured event objects (`data`, `event`, `id`, `retry`).
pub fn parse_event_stream(body: &str) -> Value {
    let mut events = Vec::new();
    let mut data_lines: Vec<String> = Vec::new();
    let mut event = Map::new();

    for raw_line in body.lines() {
        let line = raw_line.trim_end_matches(char::from(13u8));
        if line.is_empty() {
            if !data_lines.is_empty() {
                let data = data_lines.join(
                    "
",
                );
                event.insert("data".to_string(), Value::String(data));
                data_lines.clear();
            }
            if !event.is_empty() {
                events.push(Value::Object(std::mem::take(&mut event)));
            }
            continue;
        }
        if line.starts_with(':') {
            continue;
        }

        let mut parts = line.splitn(2, ':');
        let field = parts.next().unwrap_or("").trim();
        let mut value = parts.next().unwrap_or("").to_string();
        if value.starts_with(' ') {
            value = value[1..].to_string();
        }

        match field {
            "data" => data_lines.push(value),
            "event" | "id" => {
                event.insert(field.to_string(), Value::String(value));
            }
            "retry" => {
                let retry_val = value
                    .parse::<i64>()
                    .map(Value::from)
                    .unwrap_or_else(|_| Value::String(value));
                event.insert("retry".to_string(), retry_val);
            }
            _ => {}
        }
    }

    if !data_lines.is_empty() {
        let data = data_lines.join(
            "
",
        );
        event.insert("data".to_string(), Value::String(data));
    }
    if !event.is_empty() {
        events.push(Value::Object(event));
    }

    Value::Array(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_media_type_classification_and_specificity() {
        assert_eq!(
            normalize_media_type("Application/JSON; charset=utf-8"),
            "application/json"
        );
        assert!(is_sequential_json_media_type("application/jsonl"));
        assert!(is_sequential_json_media_type(
            "application/vnd.custom+ndjson"
        ));
        assert!(is_json_media_type("application/problem+json"));
        assert!(is_text_media_type("text/plain"));
        assert!(is_text_media_type("application/soap+xml"));
        assert!(is_event_stream_media_type(
            "text/event-stream; charset=utf-8"
        ));

        assert_eq!(
            media_type_specificity("application/json", "application/json"),
            Some(3)
        );
        assert_eq!(
            media_type_specificity("application/*+json", "application/problem+json"),
            Some(2)
        );
        assert_eq!(
            media_type_specificity("application/*", "application/json"),
            Some(1)
        );
        assert_eq!(
            media_type_specificity("*+json", "application/problem+json"),
            Some(1)
        );
        assert_eq!(media_type_specificity("*/*", "application/json"), Some(0));
        assert_eq!(media_type_specificity("text/*", "application/json"), None);
        assert_eq!(
            media_type_specificity("application/*+xml", "application/json"),
            None
        );
        assert_eq!(
            media_type_specificity("application/xml", "application/json"),
            None
        );
    }

    #[test]
    fn test_select_media_type_and_status() {
        let content_val = json!({
            "*/*": {"schema": {"type": "string"}},
            "application/*": {"schema": {"type": "string"}},
            "application/problem+json": {"schema": {"type": "object"}},
            "application/json": {"schema": {"type": "object"}}
        });
        let map = content_val.as_object().expect("map");
        let (matched_key, _) =
            select_media_type_for_response(map, "application/problem+json").expect("matched");
        assert_eq!(matched_key, "application/problem+json");

        let reverse_order_val = json!({
            "application/problem+json": {"schema": {"type": "object"}},
            "*/*": {"schema": {"type": "string"}}
        });
        let (rev_key, _) = select_media_type_for_response(
            reverse_order_val.as_object().expect("map"),
            "application/problem+json",
        )
        .expect("matched");
        assert_eq!(rev_key, "application/problem+json");

        let (default_key, _) = select_media_type(map).expect("selected");
        assert_eq!(default_key, "application/json");

        let plus_json_map = json!({"application/vnd+json": {}});
        assert_eq!(
            select_media_type(plus_json_map.as_object().expect("map"))
                .expect("selected")
                .0,
            "application/vnd+json"
        );

        let wildcard_json_map = json!({"application/*+json": {}});
        assert_eq!(
            select_media_type(wildcard_json_map.as_object().expect("map"))
                .expect("selected")
                .0,
            "application/*+json"
        );

        let seq_json_map = json!({"application/jsonl": {}});
        assert_eq!(
            select_media_type(seq_json_map.as_object().expect("map"))
                .expect("selected")
                .0,
            "application/jsonl"
        );

        let text_plain_map = json!({"text/plain": {}});
        assert_eq!(
            select_media_type(text_plain_map.as_object().expect("map"))
                .expect("selected")
                .0,
            "text/plain"
        );

        let xml_map = json!({"application/xml": {}});
        assert_eq!(
            select_media_type(xml_map.as_object().expect("map"))
                .expect("selected")
                .0,
            "application/xml"
        );

        let app_star_map = json!({"application/*": {}});
        assert_eq!(
            select_media_type(app_star_map.as_object().expect("map"))
                .expect("selected")
                .0,
            "application/*"
        );

        let star_map = json!({"*/*": {}});
        assert_eq!(
            select_media_type(star_map.as_object().expect("map"))
                .expect("selected")
                .0,
            "*/*"
        );

        let bin_map = json!({"image/png": {}});
        assert_eq!(
            select_media_type(bin_map.as_object().expect("map"))
                .expect("selected")
                .0,
            "image/png"
        );

        let responses = json!({
            "200": {"description": "exact"},
            "2XX": {"description": "upper range"},
            "4xx": {"description": "lower range"},
            "default": {"description": "fallback"}
        });
        assert_eq!(
            select_response_for_status(&responses, 200).expect("200")["description"],
            "exact"
        );
        assert_eq!(
            select_response_for_status(&responses, 201).expect("201")["description"],
            "upper range"
        );
        assert_eq!(
            select_response_for_status(&responses, 404).expect("404")["description"],
            "lower range"
        );
        assert_eq!(
            select_response_for_status(&responses, 500).expect("500")["description"],
            "fallback"
        );
        assert!(select_response_for_status(&json!("not-an-object"), 200).is_none());
    }

    #[test]
    fn test_resolve_refs_and_stream_parsers() {
        let openapi = json!({
            "components": {
                "responses": {
                    "NotFound": {"description": "orig", "summary": "orig_sum"},
                    "NonObj": "scalar-resp"
                },
                "headers": {
                    "X-Rate": {"description": "rate", "schema": {"type": "integer"}},
                    "NonObjHdr": "scalar-hdr"
                }
            },
            "responses": {
                "Legacy": {"description": "legacy"}
            }
        });

        let resp_ref = json!({"$ref": "#/components/responses/NotFound", "description": "override", "summary": "sum_override"});
        let resolved_resp = resolve_response_ref(&resp_ref, &openapi);
        assert_eq!(resolved_resp["description"], "override");
        assert_eq!(resolved_resp["summary"], "sum_override");

        let resp_non_obj = json!({"$ref": "#/components/responses/NonObj"});
        assert_eq!(
            resolve_response_ref(&resp_non_obj, &openapi),
            json!("scalar-resp")
        );

        let legacy_ref = json!({"$ref": "#/responses/Legacy"});
        assert_eq!(
            resolve_response_ref(&legacy_ref, &openapi)["description"],
            "legacy"
        );

        let unknown_ref = json!({"$ref": "#/other/Unknown"});
        assert_eq!(resolve_response_ref(&unknown_ref, &openapi), unknown_ref);

        let non_ref = json!({"description": "direct"});
        assert_eq!(resolve_response_ref(&non_ref, &openapi), non_ref);

        let hdr_ref = json!({"$ref": "#/components/headers/X-Rate", "description": "hdr_override", "summary": "hdr_sum"});
        let resolved_hdr = resolve_header_ref(&hdr_ref, &openapi);
        assert_eq!(resolved_hdr["description"], "hdr_override");
        assert_eq!(resolved_hdr["summary"], "hdr_sum");

        let hdr_ref_no_overrides = json!({"$ref": "#/components/headers/X-Rate"});
        let resolved_hdr_plain = resolve_header_ref(&hdr_ref_no_overrides, &openapi);
        assert_eq!(resolved_hdr_plain["description"], "rate");

        let hdr_non_obj = json!({"$ref": "#/components/headers/NonObjHdr"});
        assert_eq!(
            resolve_header_ref(&hdr_non_obj, &openapi),
            json!("scalar-hdr")
        );

        assert_eq!(resolve_header_ref(&unknown_ref, &openapi), unknown_ref);
        assert_eq!(resolve_header_ref(&non_ref, &openapi), non_ref);

        let seq_lines = r#"{"a":1}
not-json

{"b":2}"#;
        assert_eq!(
            parse_sequential_json(seq_lines),
            json!([{"a": 1}, {"b": 2}])
        );

        let seq_rs = format!("\u{1e}{}\u{1e}{}", r#"{"a":1}"#, r#"{"b":2}"#);
        assert_eq!(parse_sequential_json(&seq_rs), json!([{"a": 1}, {"b": 2}]));

        let sse = ": comment
event: update
id: 42
retry: 1000
unknown: ignored
data: line1
data:line2

event: nodata

retry: bad_int
data: second
";
        let events = parse_event_stream(sse);
        let expected_data = format!("line1{}line2", char::from(10u8));
        assert_eq!(
            events,
            json!([
                {"event": "update", "id": "42", "retry": 1000, "data": expected_data},
                {"event": "nodata"},
                {"retry": "bad_int", "data": "second"}
            ])
        );

        let sse_empty_trailing = "\n\nevent: ping\n\n";
        assert_eq!(
            parse_event_stream(sse_empty_trailing),
            json!([{"event": "ping"}])
        );
    }
}
