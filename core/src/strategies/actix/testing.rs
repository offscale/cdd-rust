#![deny(missing_docs)]

//! # Test Generation
//!
//! Logic for generating integration test code helpers and methods.

use crate::oas::models::{EncodingInfo, ExampleValue, ParamStyle};

/// Returns imports for test files.
pub fn test_imports() -> String {
    let mut code = String::new();
    code.push_str("#![allow(unused_imports, unused_variables, dead_code)]\n\n");
    code.push_str("use actix_web::{test, App, web};\n");
    code.push_str("use serde_json::Value;\n");
    code.push_str("use std::fs;\n");
    code.push_str("use jsonschema::{Draft, JSONSchema};\n\n");
    code
}

/// Generates test function signature.
pub fn test_fn_signature(fn_name: &str) -> String {
    format!("#[actix_web::test]\nasync fn {}() {{", fn_name)
}

/// Generates app init code.
pub fn test_app_init(app_factory: &str) -> String {
    format!(
        "    let app = test::init_service({}(App::new())).await;",
        app_factory
    )
}

/// Generates body setup stub.
pub fn test_body_setup_code(body: &crate::oas::RequestBodyDefinition) -> String {
    match body.format {
        crate::oas::BodyFormat::Json => json_body_setup(body),
        crate::oas::BodyFormat::Form => form_body_setup(body),
        crate::oas::BodyFormat::Multipart => multipart_body_setup(body),
        crate::oas::BodyFormat::Text => text_body_setup(body),
        crate::oas::BodyFormat::Binary => binary_body_setup(body),
    }
}

fn json_body_setup(body: &crate::oas::RequestBodyDefinition) -> String {
    if let Some(example) = &body.example {
        if example.is_serialized() {
            let payload = serialized_payload_expr(example);
            return format!(
                "        .insert_header((\"Content-Type\", \"{}\"))\n        .set_payload({})\n",
                body.media_type, payload
            );
        }
        let normalized = normalize_media_type(&body.media_type);
        let json_expr = json_value_expr(&example.value);
        if normalized == "application/json" {
            return format!("        .set_json({})\n", json_expr);
        }

        let payload = json_string_expr(&example.value);
        return format!(
            "        .insert_header((\"Content-Type\", \"{}\"))\n        .set_payload({})\n",
            body.media_type, payload
        );
    }

    if normalize_media_type(&body.media_type) == "application/json" {
        return "        .set_json(serde_json::json!({ \"dummy\": \"value\" }))\n".to_string();
    }

    let payload = json_payload_for_media_type(&body.media_type);
    format!(
        "        .insert_header((\"Content-Type\", \"{}\"))\n        .set_payload({})\n",
        body.media_type, payload
    )
}

fn form_body_setup(body: &crate::oas::RequestBodyDefinition) -> String {
    if let Some(example) = &body.example {
        if example.is_serialized() {
            let payload = serialized_payload_expr(example);
            return format!(
                "        .insert_header((\"Content-Type\", \"{}\"))\n        .set_payload({})\n",
                body.media_type, payload
            );
        }
        if normalize_media_type(&body.media_type) == "application/x-www-form-urlencoded" {
            if let Some(map) = example.value.as_object() {
                let payload = serialize_form_urlencoded(map, body.encoding.as_ref());
                let payload_lit = rust_string_literal(&payload);
                return format!(
                    "        .insert_header((\"Content-Type\", \"{}\"))\n        .set_payload({})\n",
                    body.media_type, payload_lit
                );
            }
        }

        let form_expr = json_value_expr(&example.value);
        return format!("        .set_form(&{})\n", form_expr);
    }

    if normalize_media_type(&body.media_type) == "application/x-www-form-urlencoded" {
        let map = dummy_form_map(body.encoding.as_ref());
        let payload = serialize_form_urlencoded(&map, body.encoding.as_ref());
        let payload_lit = rust_string_literal(&payload);
        return format!(
            "        .insert_header((\"Content-Type\", \"{}\"))\n        .set_payload({})\n",
            body.media_type, payload_lit
        );
    }

    format!(
        "        .insert_header((\"Content-Type\", \"{}\"))\n        .set_payload(\"dummy=value\")\n",
        body.media_type
    )
}

fn multipart_body_setup(body: &crate::oas::RequestBodyDefinition) -> String {
    let media = if body.media_type.is_empty() {
        "multipart/form-data"
    } else {
        body.media_type.as_str()
    };
    let boundary = "boundary";
    let content_type = if media.to_ascii_lowercase().contains("boundary=") {
        media.to_string()
    } else {
        format!("{}; boundary={}", media, boundary)
    };

    if let Some(example) = &body.example {
        if example.is_serialized() {
            let payload = serialized_payload_expr(example);
            return format!(
                "        .insert_header((\"Content-Type\", \"{}\"))\n        .set_payload({})\n",
                content_type, payload
            );
        }

        if let Some(payload) = build_multipart_payload(body, boundary) {
            let payload_lit = rust_string_literal(&payload);
            return format!(
                "        .insert_header((\"Content-Type\", \"{}\"))\n        .set_payload({})\n",
                content_type, payload_lit
            );
        }
    }

    let payload = build_multipart_fallback_payload(body, boundary);
    let payload_lit = rust_string_literal(&payload);
    format!(
        "        .insert_header((\"Content-Type\", \"{}\"))\n        .set_payload({})\n",
        content_type, payload_lit
    )
}

fn build_multipart_payload(
    body: &crate::oas::RequestBodyDefinition,
    boundary: &str,
) -> Option<String> {
    let example = body.example.as_ref()?;
    let value = &example.value;

    match value {
        serde_json::Value::Object(map) => Some(render_named_multipart(
            map,
            body.encoding.as_ref(),
            boundary,
        )),
        serde_json::Value::Array(items) => Some(render_positional_multipart(
            items,
            body.prefix_encoding.as_ref(),
            body.item_encoding.as_ref(),
            boundary,
        )),
        _ => Some(render_single_part("payload", value, boundary, None)),
    }
}

fn build_multipart_fallback_payload(
    body: &crate::oas::RequestBodyDefinition,
    boundary: &str,
) -> String {
    if let Some(encoding) = body.encoding.as_ref() {
        if !encoding.is_empty() {
            let mut map = serde_json::Map::new();
            for (name, enc) in encoding {
                map.insert(name.clone(), dummy_value_for_encoding(enc));
            }
            return render_named_multipart(&map, Some(encoding), boundary);
        }
    }

    if body.prefix_encoding.is_some() || body.item_encoding.is_some() {
        let mut items = Vec::new();
        if let Some(prefix) = body.prefix_encoding.as_ref() {
            for enc in prefix {
                items.push(dummy_value_for_encoding(enc));
            }
        }
        if let Some(item) = body.item_encoding.as_ref() {
            items.push(dummy_value_for_encoding(item));
        }
        if items.is_empty() {
            items.push(serde_json::Value::String("value".to_string()));
        }
        return render_positional_multipart(
            &items,
            body.prefix_encoding.as_ref(),
            body.item_encoding.as_ref(),
            boundary,
        );
    }

    render_single_part(
        "payload",
        &serde_json::Value::String("value".to_string()),
        boundary,
        None,
    )
}

fn dummy_form_map(
    encoding: Option<&std::collections::HashMap<String, EncodingInfo>>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    if let Some(enc) = encoding {
        for (name, info) in enc {
            map.insert(name.clone(), dummy_value_for_encoding(info));
        }
    }
    if map.is_empty() {
        map.insert(
            "dummy".to_string(),
            serde_json::Value::String("value".to_string()),
        );
    }
    map
}

fn dummy_value_for_encoding(info: &EncodingInfo) -> serde_json::Value {
    let content_type = info
        .content_type
        .as_ref()
        .and_then(|ct| first_content_type(ct));
    if content_type
        .as_deref()
        .map(is_json_media_type)
        .unwrap_or(false)
    {
        serde_json::json!({ "dummy": "value" })
    } else {
        serde_json::Value::String("value".to_string())
    }
}

fn render_named_multipart(
    map: &serde_json::Map<String, serde_json::Value>,
    encoding: Option<&std::collections::HashMap<String, EncodingInfo>>,
    boundary: &str,
) -> String {
    let mut payload = String::new();
    for (name, value) in map {
        let enc = encoding.and_then(|m| m.get(name));
        let content_type = enc
            .and_then(|e| e.content_type.as_ref())
            .and_then(|c| first_content_type(c));
        append_multipart_part(
            &mut payload,
            boundary,
            Some(name),
            content_type.as_deref(),
            enc.map(|e| &e.headers),
            value,
        );
    }
    finalize_multipart_payload(payload, boundary)
}

fn render_positional_multipart(
    items: &[serde_json::Value],
    prefix: Option<&Vec<EncodingInfo>>,
    item: Option<&EncodingInfo>,
    boundary: &str,
) -> String {
    let mut payload = String::new();
    for (idx, value) in items.iter().enumerate() {
        let enc = prefix.and_then(|p| p.get(idx)).or(item);
        let content_type = enc
            .and_then(|e| e.content_type.as_ref())
            .and_then(|c| first_content_type(c));
        let name = format!("part{}", idx);
        append_multipart_part(
            &mut payload,
            boundary,
            Some(&name),
            content_type.as_deref(),
            enc.map(|e| &e.headers),
            value,
        );
    }
    finalize_multipart_payload(payload, boundary)
}

fn render_single_part(
    name: &str,
    value: &serde_json::Value,
    boundary: &str,
    content_type: Option<&str>,
) -> String {
    let mut payload = String::new();
    append_multipart_part(
        &mut payload,
        boundary,
        Some(name),
        content_type,
        None,
        value,
    );
    finalize_multipart_payload(payload, boundary)
}

fn append_multipart_part(
    out: &mut String,
    boundary: &str,
    name: Option<&str>,
    content_type: Option<&str>,
    headers: Option<&std::collections::HashMap<String, String>>,
    value: &serde_json::Value,
) {
    let inferred = content_type.or_else(|| infer_multipart_content_type(value));
    let body = serialize_multipart_value(value, inferred);

    out.push_str(&format!("--{}\r\n", boundary));
    if let Some(n) = name {
        out.push_str(&format!(
            "Content-Disposition: form-data; name=\"{}\"\r\n",
            n
        ));
    }
    if let Some(ct) = inferred {
        out.push_str(&format!("Content-Type: {}\r\n", ct));
    }
    append_multipart_headers(out, headers);
    out.push_str("\r\n");
    out.push_str(&body);
    out.push_str("\r\n");
}

fn append_multipart_headers(
    out: &mut String,
    headers: Option<&std::collections::HashMap<String, String>>,
) {
    let Some(headers) = headers else {
        return;
    };

    let mut names = headers.keys().collect::<Vec<_>>();
    names.sort();
    for name in names {
        if name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        let ty = headers.get(name).map(|s| s.as_str()).unwrap_or("String");
        let value = header_value_for_type(ty);
        out.push_str(&format!("{}: {}\r\n", name, value));
    }
}

fn header_value_for_type(ty: &str) -> String {
    if ty.contains("Uuid") {
        "00000000-0000-0000-0000-000000000000".to_string()
    } else if ty.contains("i32") || ty.contains("i64") || ty.contains("Integer") {
        "1".to_string()
    } else if ty.contains("bool") || ty.contains("Boolean") {
        "true".to_string()
    } else if ty.contains("Date") {
        "2023-01-01T00:00:00Z".to_string()
    } else {
        "test_val".to_string()
    }
}

fn finalize_multipart_payload(mut payload: String, boundary: &str) -> String {
    payload.push_str(&format!("--{}--", boundary));
    payload
}

fn infer_multipart_content_type(value: &serde_json::Value) -> Option<&'static str> {
    match value {
        serde_json::Value::Object(_) | serde_json::Value::Array(_) => Some("application/json"),
        serde_json::Value::String(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::Bool(_)
        | serde_json::Value::Null => Some("text/plain"),
    }
}

fn serialize_multipart_value(value: &serde_json::Value, content_type: Option<&str>) -> String {
    if content_type
        .map(|ct| ct.to_ascii_lowercase().contains("json"))
        .unwrap_or(false)
    {
        return value.to_string();
    }

    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        _ => value.to_string(),
    }
}

fn first_content_type(value: &str) -> Option<String> {
    let ct = value.split(',').next()?.trim();
    if ct.is_empty() {
        None
    } else {
        Some(ct.to_string())
    }
}

fn text_body_setup(body: &crate::oas::RequestBodyDefinition) -> String {
    let media = if body.media_type.is_empty() {
        "text/plain"
    } else {
        body.media_type.as_str()
    };
    if let Some(example) = &body.example {
        let payload = if example.is_serialized() {
            serialized_payload_expr(example)
        } else {
            text_string_expr(&example.value)
        };
        return format!(
            "        .insert_header((\"Content-Type\", \"{}\"))\n        .set_payload({})\n",
            media, payload
        );
    }
    format!(
        "        .insert_header((\"Content-Type\", \"{}\"))\n        .set_payload(\"dummy\")\n",
        media
    )
}

fn binary_body_setup(body: &crate::oas::RequestBodyDefinition) -> String {
    let media = if body.media_type.is_empty() {
        "application/octet-stream"
    } else {
        body.media_type.as_str()
    };
    if let Some(example) = &body.example {
        if example.is_serialized() {
            let payload = serialized_payload_expr(example);
            return format!(
                "        .insert_header((\"Content-Type\", \"{}\"))\n        .set_payload({})\n",
                media, payload
            );
        }
    }
    format!(
        "        .insert_header((\"Content-Type\", \"{}\"))\n        .set_payload(vec![0u8, 1u8, 2u8, 3u8])\n",
        media
    )
}

fn json_payload_for_media_type(media_type: &str) -> String {
    if is_sequential_json_media_type(media_type) {
        "\"{\\\"dummy\\\":\\\"value\\\"}\\n{\\\"dummy\\\":\\\"value\\\"}\"".to_string()
    } else {
        "\"{\\\"dummy\\\":\\\"value\\\"}\"".to_string()
    }
}

fn json_value_expr(value: &serde_json::Value) -> String {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
    let literal = rust_string_literal(&serialized);
    format!(
        "serde_json::from_str::<serde_json::Value>({}).unwrap()",
        literal
    )
}

fn json_string_expr(value: &serde_json::Value) -> String {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
    rust_string_literal(&serialized)
}

fn text_string_expr(value: &serde_json::Value) -> String {
    let payload = match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => value.to_string(),
    };
    rust_string_literal(&payload)
}

fn example_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => value.to_string(),
    }
}

fn serialized_payload_expr(example: &ExampleValue) -> String {
    let payload = example_to_string(&example.value);
    rust_string_literal(&payload)
}

fn rust_string_literal(value: &str) -> String {
    format!("{:?}", value)
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
        || normalized == "application/*+json"
        || normalized.ends_with("+json")
        || is_sequential_json_media_type(&normalized)
}

fn serialize_form_urlencoded(
    map: &serde_json::Map<String, serde_json::Value>,
    encoding: Option<&std::collections::HashMap<String, EncodingInfo>>,
) -> String {
    let mut pairs = Vec::new();

    for (name, value) in map {
        let enc = encoding.and_then(|e| e.get(name));
        let style = enc
            .and_then(|e| e.style.clone())
            .unwrap_or(ParamStyle::Form);
        let explode = enc
            .and_then(|e| e.explode)
            .unwrap_or(matches!(style, ParamStyle::Form));
        let allow_reserved = enc.and_then(|e| e.allow_reserved).unwrap_or(false);
        let content_type = enc
            .and_then(|e| e.content_type.as_ref())
            .and_then(|ct| first_content_type(ct));

        if content_type
            .as_deref()
            .map(is_json_media_type)
            .unwrap_or(false)
        {
            let json_payload = value.to_string();
            pairs.push(format!(
                "{}={}",
                encode_form_component(name, allow_reserved),
                encode_form_component(&json_payload, allow_reserved)
            ));
            continue;
        }

        pairs.extend(serialize_form_param(
            name,
            value,
            &style,
            explode,
            allow_reserved,
        ));
    }

    pairs.join("&")
}

fn serialize_form_param(
    name: &str,
    value: &serde_json::Value,
    style: &ParamStyle,
    explode: bool,
    allow_reserved: bool,
) -> Vec<String> {
    match value {
        serde_json::Value::Array(items) => {
            serialize_form_array(name, style, explode, allow_reserved, items)
        }
        serde_json::Value::Object(map) => {
            serialize_form_object(name, style, explode, allow_reserved, map)
        }
        _ => vec![format!(
            "{}={}",
            encode_form_component(name, allow_reserved),
            encode_form_component(&example_to_string(value), allow_reserved)
        )],
    }
}

fn serialize_form_array(
    name: &str,
    style: &ParamStyle,
    explode: bool,
    allow_reserved: bool,
    items: &[serde_json::Value],
) -> Vec<String> {
    if items.is_empty() {
        return Vec::new();
    }

    match style {
        ParamStyle::SpaceDelimited => {
            let joined = items
                .iter()
                .map(|v| encode_form_component(&example_to_string(v), allow_reserved))
                .collect::<Vec<_>>()
                .join("%20");
            vec![format!(
                "{}={}",
                encode_form_component(name, allow_reserved),
                joined
            )]
        }
        ParamStyle::PipeDelimited => {
            let joined = items
                .iter()
                .map(|v| encode_form_component(&example_to_string(v), allow_reserved))
                .collect::<Vec<_>>()
                .join("%7C");
            vec![format!(
                "{}={}",
                encode_form_component(name, allow_reserved),
                joined
            )]
        }
        _ => {
            if explode {
                items
                    .iter()
                    .map(|v| {
                        format!(
                            "{}={}",
                            encode_form_component(name, allow_reserved),
                            encode_form_component(&example_to_string(v), allow_reserved)
                        )
                    })
                    .collect()
            } else {
                let joined = items
                    .iter()
                    .map(|v| encode_form_component(&example_to_string(v), allow_reserved))
                    .collect::<Vec<_>>()
                    .join(",");
                vec![format!(
                    "{}={}",
                    encode_form_component(name, allow_reserved),
                    joined
                )]
            }
        }
    }
}

fn serialize_form_object(
    name: &str,
    style: &ParamStyle,
    explode: bool,
    allow_reserved: bool,
    map: &serde_json::Map<String, serde_json::Value>,
) -> Vec<String> {
    if map.is_empty() {
        return Vec::new();
    }

    match style {
        ParamStyle::DeepObject => map
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    encode_form_deep_object_name(name, k, allow_reserved),
                    encode_form_component(&example_to_string(v), allow_reserved)
                )
            })
            .collect(),
        ParamStyle::SpaceDelimited => {
            let joined = map
                .iter()
                .flat_map(|(k, v)| {
                    [
                        encode_form_component(k, allow_reserved),
                        encode_form_component(&example_to_string(v), allow_reserved),
                    ]
                })
                .collect::<Vec<_>>()
                .join("%20");
            vec![format!(
                "{}={}",
                encode_form_component(name, allow_reserved),
                joined
            )]
        }
        ParamStyle::PipeDelimited => {
            let joined = map
                .iter()
                .flat_map(|(k, v)| {
                    [
                        encode_form_component(k, allow_reserved),
                        encode_form_component(&example_to_string(v), allow_reserved),
                    ]
                })
                .collect::<Vec<_>>()
                .join("%7C");
            vec![format!(
                "{}={}",
                encode_form_component(name, allow_reserved),
                joined
            )]
        }
        _ => {
            if explode {
                map.iter()
                    .map(|(k, v)| {
                        format!(
                            "{}={}",
                            encode_form_component(k, allow_reserved),
                            encode_form_component(&example_to_string(v), allow_reserved)
                        )
                    })
                    .collect()
            } else {
                let joined = map
                    .iter()
                    .flat_map(|(k, v)| {
                        [
                            encode_form_component(k, allow_reserved),
                            encode_form_component(&example_to_string(v), allow_reserved),
                        ]
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                vec![format!(
                    "{}={}",
                    encode_form_component(name, allow_reserved),
                    joined
                )]
            }
        }
    }
}

fn encode_form_deep_object_name(name: &str, key: &str, allow_reserved: bool) -> String {
    let name = encode_form_component(name, allow_reserved);
    let key = encode_form_component(key, allow_reserved);
    format!("{}%5B{}%5D", name, key)
}

fn encode_form_component(input: &str, allow_reserved: bool) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    for &b in bytes {
        match b {
            b' ' => out.push('+'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'*' => {
                out.push(b as char)
            }
            _ if allow_reserved && is_reserved(b) => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn is_reserved(b: u8) -> bool {
    matches!(
        b,
        b':' | b'/'
            | b'?'
            | b'#'
            | b'['
            | b']'
            | b'@'
            | b'!'
            | b'$'
            | b'&'
            | b'\''
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b';'
            | b'='
    )
}

/// Generates request builder chain.
pub fn test_request_builder(method: &str, uri: &str, body_setup: &str) -> String {
    let method_lower = method.to_lowercase();
    let builder_call = match method_lower.as_str() {
        "get" | "post" | "put" | "delete" | "patch" => format!("{}()", method_lower),
        "query" => "method(actix_web::http::Method::from_bytes(b\"QUERY\").unwrap())".to_string(),
        _ => format!(
            "method(actix_web::http::Method::from_bytes(b\"{}\").unwrap())",
            method.to_uppercase()
        ),
    };

    format!(
        "    let req = test::TestRequest::{}.uri(\"{}\")\n{}        .to_request();",
        builder_call, uri, body_setup
    )
}

/// Generates call service line.
pub fn test_api_call() -> String {
    "    let resp = test::call_service(&app, req).await;".to_string()
}

/// Generates assertions.
pub fn test_assertion() -> String {
    "    assert_ne!(resp.status(), actix_web::http::StatusCode::NOT_FOUND, \"Route should exist\");"
        .to_string()
}

/// Generates validation helper function with status- and content-type-aware response matching.
pub fn test_validation_helper() -> String {
    r##"
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

fn is_json_media_type(media_type: &str) -> bool {
    let normalized = normalize_media_type(media_type);
    normalized == "application/json"
        || normalized == "application/*+json"
        || normalized.ends_with("+json")
        || is_sequential_json_media_type(&normalized)
}

fn is_text_media_type(media_type: &str) -> bool {
    let normalized = normalize_media_type(media_type);
    normalized.starts_with("text/")
        || normalized == "application/xml"
        || normalized == "text/xml"
        || normalized.ends_with("+xml")
}

fn is_event_stream_media_type(media_type: &str) -> bool {
    normalize_media_type(media_type) == "text/event-stream"
}

fn media_type_specificity(pattern: &str, actual: &str) -> Option<i32> {
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
        let score = if !prefix.is_empty() && !suffix.is_empty() { 2 } else { 1 };
        return Some(score);
    }
    None
}

fn select_media_type_for_response(
    content: &serde_json::Map<String, serde_json::Value>,
    content_type: &str,
) -> Option<(String, &serde_json::Value)> {
    let mut best: Option<(String, &serde_json::Value, i32)> = None;
    for (key, value) in content.iter() {
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

fn select_media_type(
    content: &serde_json::Map<String, serde_json::Value>,
) -> Option<(String, &serde_json::Value)> {
    if let Some(media) = content.get("application/json") {
        return Some(("application/json".to_string(), media));
    }

    if let Some((key, media)) = content.iter().find(|(k, _)| {
        let normalized = normalize_media_type(k);
        normalized.ends_with("+json")
            || normalized == "application/*+json"
            || is_sequential_json_media_type(&normalized)
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

fn select_response_for_status(
    responses: &serde_json::Value,
    status: u16,
) -> Option<&serde_json::Value> {
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

fn schema_draft_from_uri(uri: &str) -> Option<Draft> {
    let normalized = uri.trim_end_matches('#');
    match normalized {
        "https://spec.openapis.org/oas/3.1/dialect/base" => Some(Draft::Draft202012),
        "https://json-schema.org/draft/2020-12/schema" => Some(Draft::Draft202012),
        "https://json-schema.org/draft/2019-09/schema" => Some(Draft::Draft201909),
        "http://json-schema.org/draft-07/schema" => Some(Draft::Draft7),
        "http://json-schema.org/draft-04/schema" => Some(Draft::Draft4),
        _ => None,
    }
}

fn resolve_schema_draft(
    schema: &serde_json::Value,
    openapi: &serde_json::Value,
) -> Option<Draft> {
    if let Some(uri) = schema.get("$schema").and_then(|v| v.as_str()) {
        if let Some(draft) = schema_draft_from_uri(uri) {
            return Some(draft);
        }
    }

    if let Some(uri) = openapi.get("jsonSchemaDialect").and_then(|v| v.as_str()) {
        if let Some(draft) = schema_draft_from_uri(uri) {
            return Some(draft);
        }
    }

    if let Some(version) = openapi.get("openapi").and_then(|v| v.as_str()) {
        if version.starts_with("3.1") || version.starts_with("3.2") {
            return Some(Draft::Draft202012);
        }
        if version.starts_with("3.0") {
            return Some(Draft::Draft4);
        }
    }

    if openapi.get("swagger").and_then(|v| v.as_str()).is_some() {
        return Some(Draft::Draft4);
    }

    None
}

fn compile_schema(
    schema: &serde_json::Value,
    openapi: &serde_json::Value,
) -> Result<JSONSchema, jsonschema::CompilationError> {
    let opts = if let Some(draft) = resolve_schema_draft(schema, openapi) {
        JSONSchema::options().with_draft(draft)
    } else {
        JSONSchema::options()
    };
    opts.compile(schema)
}

fn resolve_response_ref(resp_def: &serde_json::Value, openapi: &serde_json::Value) -> serde_json::Value {
    let Some(ref_str) = resp_def.get("$ref").and_then(|v| v.as_str()) else {
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
    if let serde_json::Value::Object(map) = &mut merged {
        if let Some(summary) = resp_def.get("summary") {
            map.insert("summary".to_string(), summary.clone());
        }
        if let Some(description) = resp_def.get("description") {
            map.insert("description".to_string(), description.clone());
        }
    }
    merged
}

fn resolve_header_ref(header_def: &serde_json::Value, openapi: &serde_json::Value) -> serde_json::Value {
    let Some(ref_str) = header_def.get("$ref").and_then(|v| v.as_str()) else {
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
    if let serde_json::Value::Object(map) = &mut merged {
        if let Some(summary) = header_def.get("summary") {
            map.insert("summary".to_string(), summary.clone());
        }
        if let Some(description) = header_def.get("description") {
            map.insert("description".to_string(), description.clone());
        }
    }
    merged
}

fn header_values(headers: &actix_web::http::HeaderMap, name: &str) -> Vec<String> {
    headers
        .get_all(name)
        .iter()
        .filter_map(|value| value.to_str().ok().map(|s| s.to_string()))
        .collect()
}

fn extract_header_meta(
    resolved: &serde_json::Value,
) -> Option<(serde_json::Value, bool, bool)> {
    if let Some(schema) = resolved.get("schema") {
        let explode = resolved
            .get("explode")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        return Some((schema.clone(), false, explode));
    }

    let content = resolved.get("content").and_then(|c| c.as_object())?;
    let (_, media_def) = select_media_type(content)?;
    let schema = media_def.get("schema")?.clone();
    Some((schema, true, false))
}

fn split_header_parts(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn schema_type_hint(schema: &serde_json::Value) -> Option<String> {
    match schema.get("type") {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .find(|s| *s != "null")
            .map(|s| s.to_string()),
        _ => None,
    }
}

fn parse_bool_header(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn validate_header_value(name: &str, schema: &serde_json::Value, value: &str) {
    if schema.as_bool() == Some(false) {
        panic!("Response header '{}' is not allowed by schema: false", name);
    }

    if let Some(enum_vals) = schema.get("enum").and_then(|v| v.as_array()) {
        let allowed: Vec<String> = enum_vals
            .iter()
            .map(|v| {
                if let Some(s) = v.as_str() {
                    s.to_string()
                } else {
                    v.to_string()
                }
            })
            .collect();
        if !allowed.iter().any(|v| v == value) {
            panic!(
                "Response header '{}' value '{}' not in enum {:?}",
                name, value, allowed
            );
        }
        return;
    }

    let Some(kind) = schema_type_hint(schema) else {
        return;
    };

    match kind.as_str() {
        "integer" => {
            if value.parse::<i64>().is_err() {
                panic!(
                    "Response header '{}' should be integer, got '{}'",
                    name, value
                );
            }
        }
        "number" => {
            if value.parse::<f64>().is_err() {
                panic!(
                    "Response header '{}' should be number, got '{}'",
                    name, value
                );
            }
        }
        "boolean" => {
            if parse_bool_header(value).is_none() {
                panic!(
                    "Response header '{}' should be boolean, got '{}'",
                    name, value
                );
            }
        }
        "string" => {}
        _ => {}
    }
}

fn validate_header_values(
    name: &str,
    schema: &serde_json::Value,
    values: &[String],
    explode: bool,
    is_content: bool,
) {
    if schema.as_bool() == Some(false) {
        panic!("Response header '{}' is not allowed by schema: false", name);
    }

    if values.is_empty() {
        return;
    }

    if name.eq_ignore_ascii_case("set-cookie") {
        validate_set_cookie_values(name, schema, values);
        return;
    }

    let kind = schema_type_hint(schema);
    if !is_content {
        if let Some(kind) = kind.as_deref() {
            let joined = values.join(",");
            if kind == "array" {
                let items_schema = schema.get("items").unwrap_or(schema);
                for part in split_header_parts(&joined) {
                    validate_header_value(name, items_schema, &part);
                }
                return;
            }
            if kind == "object" {
                let props = schema.get("properties").and_then(|v| v.as_object());
                let additional = schema.get("additionalProperties");
                if explode {
                    for pair in split_header_parts(&joined) {
                        if let Some((k, v)) = pair.split_once('=') {
                            if let Some(prop_schema) = props.and_then(|p| p.get(k)) {
                                validate_header_value(name, prop_schema, v);
                            } else if let Some(additional) = additional {
                                if additional == &serde_json::Value::Bool(false) {
                                    panic!(
                                        "Response header '{}' contains unknown field '{}'",
                                        name, k
                                    );
                                }
                                validate_header_value(name, additional, v);
                            }
                        }
                    }
                } else {
                    let parts = split_header_parts(&joined);
                    if parts.len() % 2 != 0 {
                        panic!(
                            "Response header '{}' object serialization is malformed",
                            name
                        );
                    }
                    for chunk in parts.chunks(2) {
                        let key = &chunk[0];
                        let val = &chunk[1];
                        if let Some(prop_schema) = props.and_then(|p| p.get(key)) {
                            validate_header_value(name, prop_schema, val);
                        } else if let Some(additional) = additional {
                            if additional == &serde_json::Value::Bool(false) {
                                panic!(
                                    "Response header '{}' contains unknown field '{}'",
                                    name, key
                                );
                            }
                            validate_header_value(name, additional, val);
                        }
                    }
                }
                return;
            }
        }
    }

    for value in values {
        validate_header_value(name, schema, value);
    }
}

fn validate_set_cookie_values(name: &str, schema: &serde_json::Value, values: &[String]) {
    let kind = schema_type_hint(schema);
    match kind.as_deref() {
        Some("array") => {
            let items_schema = schema.get("items").unwrap_or(schema);
            for value in values {
                validate_header_value(name, items_schema, value);
            }
        }
        Some("object") => {
            let props = schema.get("properties").and_then(|v| v.as_object());
            let additional = schema.get("additionalProperties");
            for value in values {
                let Some((k, v)) = value.split_once('=') else {
                    panic!(
                        "Response header '{}' Set-Cookie value '{}' is malformed",
                        name, value
                    );
                };
                let key = k.trim();
                if let Some(prop_schema) = props.and_then(|p| p.get(key)) {
                    validate_header_value(name, prop_schema, v);
                } else if let Some(additional) = additional {
                    if additional == &serde_json::Value::Bool(false) {
                        panic!(
                            "Response header '{}' contains unknown field '{}'",
                            name, key
                        );
                    }
                    validate_header_value(name, additional, v);
                }
            }
        }
        _ => {
            for value in values {
                validate_header_value(name, schema, value);
            }
        }
    }
}

fn validate_required_headers(
    resp_def: &serde_json::Value,
    headers: &actix_web::http::HeaderMap,
    openapi: &serde_json::Value,
) {
    let Some(header_map) = resp_def.get("headers").and_then(|h| h.as_object()) else {
        return;
    };

    for (name, header_def) in header_map.iter() {
        if name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        let resolved = resolve_header_ref(header_def, openapi);
        let required = resolved
            .get("required")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let values = header_values(headers, name);
        if required && values.is_empty() {
            panic!("Required response header '{}' is missing", name);
        }
        if let Some((schema, is_content, explode)) = extract_header_meta(&resolved) {
            if !values.is_empty() {
                validate_header_values(name, &schema, &values, explode, is_content);
            }
        }
    }
}

fn parse_sequential_json(body: &str) -> serde_json::Value {
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
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            items.push(val);
        }
    }

    serde_json::Value::Array(items)
}

fn parse_event_stream(body: &str) -> serde_json::Value {
    let mut events = Vec::new();
    let mut data_lines: Vec<String> = Vec::new();
    let mut event = serde_json::Map::new();

    for raw_line in body.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            if !data_lines.is_empty() {
                let data = data_lines.join("\n");
                event.insert("data".to_string(), serde_json::Value::String(data));
                data_lines.clear();
            }
            if !event.is_empty() {
                events.push(serde_json::Value::Object(std::mem::take(&mut event)));
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
                event.insert(field.to_string(), serde_json::Value::String(value));
            }
            "retry" => {
                let retry_val = value
                    .parse::<i64>()
                    .map(serde_json::Value::from)
                    .unwrap_or_else(|_| serde_json::Value::String(value));
                event.insert("retry".to_string(), retry_val);
            }
            _ => {}
        }
    }

    if !data_lines.is_empty() {
        let data = data_lines.join("\n");
        event.insert("data".to_string(), serde_json::Value::String(data));
    }
    if !event.is_empty() {
        events.push(serde_json::Value::Object(event));
    }

    serde_json::Value::Array(events)
}

/// Helper to validate response body against OpenAPI schema.
async fn validate_response(resp: actix_web::dev::ServiceResponse, method: &str, path_template: &str) {
    use actix_web::body::MessageBody;
    let status = resp.status();
    let status_code = status.as_u16();
    let headers = resp.headers().clone();
    let content_type = headers
        .get(actix_web::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    let body_bytes = resp.into_body().try_into_bytes().expect("Failed to read response body");
    let yaml_content = fs::read_to_string(OPENAPI_PATH).expect("Failed to read openapi.yaml");
    let openapi: serde_json::Value = serde_yaml::from_str(&yaml_content).expect("Failed to parse OpenAPI");

    let method_key = method.to_lowercase();
    let operation = openapi.get("paths")
        .and_then(|p| p.get(path_template))
        .and_then(|path_item| path_item.get(&method_key));

    if let Some(op) = operation {
        let responses = op.get("responses");
        let response = responses.and_then(|r| select_response_for_status(r, status_code));
        if response.is_none() {
            panic!(
                "Response status {} not documented for {} {}",
                status_code, method, path_template
            );
        }
        if let Some(resp_def) = response {
            let resp_def = resolve_response_ref(resp_def, &openapi);
            validate_required_headers(&resp_def, &headers, &openapi);
            let schema_swagger2 = resp_def.get("schema");

            if let Some(schema) = schema_swagger2 {
                let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null);
                match compile_schema(schema, &openapi) {
                    Ok(validator) => {
                        if let Err(errors) = validator.validate(&body_json) {
                            let err_msgs: Vec<String> = errors.map(|e| e.to_string()).collect();
                            panic!("Response schema validation failed: {}", err_msgs.join("\n"));
                        }
                    }
                    Err(e) => panic!("Failed to compile JSON Schema: {}", e),
                }
                return;
            }

            let content = resp_def.get("content").and_then(|c| c.as_object());
            if let Some(content_map) = content {
                let selected = content_type
                    .as_deref()
                    .and_then(|ct| select_media_type_for_response(content_map, ct));
                if let Some((media_type, media_def)) = selected.or_else(|| select_media_type(content_map)) {
                    let schema_value = media_def.get("schema").cloned().or_else(|| {
                        let normalized = normalize_media_type(&media_type);
                        if is_sequential_json_media_type(&normalized)
                            || is_event_stream_media_type(&normalized)
                        {
                            media_def.get("itemSchema").map(|item| {
                                serde_json::json!({
                                    "type": "array",
                                    "items": item.clone()
                                })
                            })
                        } else {
                            None
                        }
                    });

                    if let Some(schema) = schema_value {
                        let body_json = if is_json_media_type(&media_type) {
                            let body_str = String::from_utf8_lossy(&body_bytes);
                            if is_sequential_json_media_type(&normalize_media_type(&media_type)) {
                                parse_sequential_json(&body_str)
                            } else {
                                serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null)
                            }
                        } else if is_event_stream_media_type(&media_type) {
                            let body_str = String::from_utf8_lossy(&body_bytes);
                            parse_event_stream(&body_str)
                        } else if is_text_media_type(&media_type) {
                            let body_str = String::from_utf8_lossy(&body_bytes).to_string();
                            serde_json::Value::String(body_str)
                        } else {
                            return;
                        };

                        match compile_schema(&schema, &openapi) {
                            Ok(validator) => {
                                if let Err(errors) = validator.validate(&body_json) {
                                    let err_msgs: Vec<String> = errors.map(|e| e.to_string()).collect();
                                    panic!("Response schema validation failed: {}", err_msgs.join("\n"));
                                }
                            }
                            Err(e) => panic!("Failed to compile JSON Schema: {}", e),
                        }
                    }
                }
            }
        }
    }
}
"##
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_imports_and_signature() {
        let imports = test_imports();
        assert!(imports.contains("use actix_web::{test, App, web};"));
        assert!(imports.contains("use jsonschema::{Draft, JSONSchema};"));

        let sig = test_fn_signature("test_ping");
        assert!(sig.contains("#[actix_web::test]"));
        assert!(sig.contains("async fn test_ping() {"));
    }

    #[test]
    fn test_app_init_and_body_setup() {
        let init = test_app_init("crate::create_app");
        assert!(init.contains("test::init_service"));
        assert!(init.contains("crate::create_app"));

        let body = test_body_setup_code(&crate::oas::RequestBodyDefinition {
            ty: "Payload".into(),
            description: None,
            media_type: "application/json".into(),
            format: crate::oas::BodyFormat::Json,
            required: true,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
            example: None,
        });
        assert!(body.contains("set_json"));
    }

    #[test]
    fn test_request_builder_variants() {
        let body = "        .set_json(serde_json::json!({}))\n";

        let get_req = test_request_builder("GET", "/users", body);
        assert!(get_req.contains("TestRequest::get()"));

        let query_req = test_request_builder("QUERY", "/search", body);
        assert!(
            query_req.contains("method(actix_web::http::Method::from_bytes(b\"QUERY\").unwrap())")
        );

        let custom_req = test_request_builder("PROPFIND", "/files", body);
        assert!(custom_req.contains("from_bytes(b\"PROPFIND\")"));
    }

    #[test]
    fn test_body_setup_variants() {
        let text_body = test_body_setup_code(&crate::oas::RequestBodyDefinition {
            ty: "String".into(),
            description: None,
            media_type: "text/plain".into(),
            format: crate::oas::BodyFormat::Text,
            required: true,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
            example: None,
        });
        assert!(text_body.contains("text/plain"));
        assert!(text_body.contains("set_payload(\"dummy\")"));

        let binary_body = test_body_setup_code(&crate::oas::RequestBodyDefinition {
            ty: "Vec<u8>".into(),
            description: None,
            media_type: "application/octet-stream".into(),
            format: crate::oas::BodyFormat::Binary,
            required: true,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
            example: None,
        });
        assert!(binary_body.contains("application/octet-stream"));
        assert!(binary_body.contains("set_payload(vec![0u8"));
    }

    #[test]
    fn test_body_setup_uses_example_json() {
        let body = test_body_setup_code(&crate::oas::RequestBodyDefinition {
            ty: "Payload".into(),
            description: None,
            media_type: "application/json".into(),
            format: crate::oas::BodyFormat::Json,
            required: true,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
            example: Some(crate::oas::ExampleValue::data(serde_json::json!({
                "hello": "world"
            }))),
        });
        assert!(body.contains("from_str::<serde_json::Value>"));
        assert!(body.contains("hello"));
    }

    #[test]
    fn test_body_setup_uses_serialized_example() {
        let body = test_body_setup_code(&crate::oas::RequestBodyDefinition {
            ty: "Payload".into(),
            description: None,
            media_type: "application/json".into(),
            format: crate::oas::BodyFormat::Json,
            required: true,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
            example: Some(crate::oas::ExampleValue::serialized(serde_json::json!(
                "{\"ok\":true}"
            ))),
        });
        assert!(body.contains("set_payload"));
        assert!(body.contains("{\\\"ok\\\":true}"));
    }

    #[test]
    fn test_multipart_body_setup_uses_serialized_example() {
        let body = test_body_setup_code(&crate::oas::RequestBodyDefinition {
            ty: "Upload".into(),
            description: None,
            media_type: "multipart/form-data".into(),
            format: crate::oas::BodyFormat::Multipart,
            required: true,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
            example: Some(crate::oas::ExampleValue::serialized(serde_json::json!(
                "--boundary\r\nContent-Disposition: form-data; name=\"field\"\r\n\r\nvalue\r\n--boundary--"
            ))),
        });
        assert!(body.contains("Content-Type"));
        assert!(body.contains("set_payload"));
        assert!(body.contains("--boundary"));
    }

    #[test]
    fn test_multipart_body_setup_builds_named_parts() {
        let mut enc = std::collections::HashMap::new();
        enc.insert(
            "file".to_string(),
            crate::oas::models::EncodingInfo {
                content_type: Some("image/png".to_string()),
                headers: std::collections::HashMap::new(),
                style: None,
                explode: None,
                allow_reserved: None,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
            },
        );
        let body = test_body_setup_code(&crate::oas::RequestBodyDefinition {
            ty: "Upload".into(),
            description: None,
            media_type: "multipart/form-data".into(),
            format: crate::oas::BodyFormat::Multipart,
            required: true,
            encoding: Some(enc),
            prefix_encoding: None,
            item_encoding: None,
            example: Some(crate::oas::ExampleValue::data(serde_json::json!({
                "file": "pngbytes",
                "meta": { "id": 1 }
            }))),
        });
        assert!(body.contains("Content-Disposition: form-data; name=\\\"file\\\""));
        assert!(body.contains("Content-Type: image/png"));
        assert!(body.contains("Content-Disposition: form-data; name=\\\"meta\\\""));
        assert!(body.contains("Content-Type: application/json"));
        assert!(body.contains("\\\"id\\\":1"));
    }

    #[test]
    fn test_multipart_body_setup_includes_encoding_headers() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Trace-Id".to_string(), "Uuid".to_string());
        headers.insert("Content-Type".to_string(), "String".to_string());
        let mut enc = std::collections::HashMap::new();
        enc.insert(
            "file".to_string(),
            crate::oas::models::EncodingInfo {
                content_type: Some("image/png".to_string()),
                headers,
                style: None,
                explode: None,
                allow_reserved: None,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
            },
        );
        let body = test_body_setup_code(&crate::oas::RequestBodyDefinition {
            ty: "Upload".into(),
            description: None,
            media_type: "multipart/form-data".into(),
            format: crate::oas::BodyFormat::Multipart,
            required: true,
            encoding: Some(enc),
            prefix_encoding: None,
            item_encoding: None,
            example: Some(crate::oas::ExampleValue::data(serde_json::json!({
                "file": "pngbytes"
            }))),
        });
        assert!(body.contains("X-Trace-Id"));
        assert_eq!(body.matches("Content-Type: image/png").count(), 1);
    }

    #[test]
    fn test_multipart_body_setup_default_payload() {
        let mut enc = std::collections::HashMap::new();
        enc.insert(
            "file".to_string(),
            crate::oas::models::EncodingInfo {
                content_type: Some("image/png".to_string()),
                headers: std::collections::HashMap::new(),
                style: None,
                explode: None,
                allow_reserved: None,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
            },
        );
        let body = test_body_setup_code(&crate::oas::RequestBodyDefinition {
            ty: "Upload".into(),
            description: None,
            media_type: "multipart/form-data".into(),
            format: crate::oas::BodyFormat::Multipart,
            required: true,
            encoding: Some(enc),
            prefix_encoding: None,
            item_encoding: None,
            example: None,
        });
        assert!(body.contains("multipart/form-data"));
        assert!(body.contains("Content-Disposition: form-data; name=\\\"file\\\""));
        assert!(body.contains("Content-Type: image/png"));
    }

    #[test]
    fn test_form_body_setup_urlencoded_example() {
        let body = test_body_setup_code(&crate::oas::RequestBodyDefinition {
            ty: "Form".into(),
            description: None,
            media_type: "application/x-www-form-urlencoded".into(),
            format: crate::oas::BodyFormat::Form,
            required: true,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
            example: Some(crate::oas::ExampleValue::data(serde_json::json!({
                "foo": "a + b",
                "bar": true
            }))),
        });
        assert!(body.contains("Content-Type"));
        assert!(body.contains("foo=a+%2B+b"));
        assert!(body.contains("bar=true"));
    }

    #[test]
    fn test_form_body_setup_urlencoded_default_payload() {
        let body = test_body_setup_code(&crate::oas::RequestBodyDefinition {
            ty: "Form".into(),
            description: None,
            media_type: "application/x-www-form-urlencoded".into(),
            format: crate::oas::BodyFormat::Form,
            required: true,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
            example: None,
        });
        assert!(body.contains("Content-Type"));
        assert!(body.contains("set_payload"));
        assert!(body.contains("dummy=value"));
    }

    #[test]
    fn test_form_body_setup_urlencoded_default_payload_uses_encoding() {
        let mut encoding = HashMap::new();
        encoding.insert(
            "payload".into(),
            EncodingInfo {
                content_type: Some("application/json".to_string()),
                headers: HashMap::new(),
                style: Some(ParamStyle::Form),
                explode: Some(true),
                allow_reserved: Some(false),
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
            },
        );
        let body = test_body_setup_code(&crate::oas::RequestBodyDefinition {
            ty: "Form".into(),
            description: None,
            media_type: "application/x-www-form-urlencoded".into(),
            format: crate::oas::BodyFormat::Form,
            required: true,
            encoding: Some(encoding),
            prefix_encoding: None,
            item_encoding: None,
            example: None,
        });
        assert!(body.contains("payload=%7B%22dummy%22%3A%22value%22%7D"));
    }

    #[test]
    fn test_form_body_setup_urlencoded_json_content_type() {
        let mut encoding = HashMap::new();
        encoding.insert(
            "payload".into(),
            EncodingInfo {
                content_type: Some("application/json".to_string()),
                headers: HashMap::new(),
                style: Some(ParamStyle::Form),
                explode: Some(true),
                allow_reserved: Some(false),
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
            },
        );
        let body = test_body_setup_code(&crate::oas::RequestBodyDefinition {
            ty: "Form".into(),
            description: None,
            media_type: "application/x-www-form-urlencoded".into(),
            format: crate::oas::BodyFormat::Form,
            required: true,
            encoding: Some(encoding),
            prefix_encoding: None,
            item_encoding: None,
            example: Some(crate::oas::ExampleValue::data(serde_json::json!({
                "payload": { "a": 1, "b": "x" }
            }))),
        });
        assert!(body.contains("payload=%7B%22a%22%3A1%2C%22b%22%3A%22x%22%7D"));
    }

    #[test]
    fn test_form_body_setup_urlencoded_allow_reserved() {
        let mut encoding = HashMap::new();
        encoding.insert(
            "path".into(),
            EncodingInfo {
                content_type: None,
                headers: HashMap::new(),
                style: Some(ParamStyle::Form),
                explode: Some(true),
                allow_reserved: Some(true),
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
            },
        );
        let body = test_body_setup_code(&crate::oas::RequestBodyDefinition {
            ty: "Form".into(),
            description: None,
            media_type: "application/x-www-form-urlencoded".into(),
            format: crate::oas::BodyFormat::Form,
            required: true,
            encoding: Some(encoding),
            prefix_encoding: None,
            item_encoding: None,
            example: Some(crate::oas::ExampleValue::data(serde_json::json!({
                "path": "a/b"
            }))),
        });
        assert!(body.contains("path=a/b"));
    }

    #[test]
    fn test_api_call_and_assertion() {
        let call = test_api_call();
        assert!(call.contains("call_service"));

        let assertion = test_assertion();
        assert!(assertion.contains("StatusCode::NOT_FOUND"));

        let helper = test_validation_helper();
        assert!(helper.contains("validate_response"));
        assert!(helper.contains("compile_schema"));
        assert!(helper.contains("resolve_schema_draft"));
        assert!(helper.contains("parse_sequential_json"));
        assert!(helper.contains("parse_event_stream"));
        assert!(helper.contains("select_media_type"));
        assert!(helper.contains("select_media_type_for_response"));
        assert!(helper.contains("select_response_for_status"));
        assert!(helper.contains("validate_required_headers"));
        assert!(helper.contains("extract_header_meta"));
        assert!(helper.contains("split_header_parts"));
        assert!(helper.contains("validate_header_value"));
        assert!(helper.contains("validate_header_values"));
        assert!(helper.contains("validate_set_cookie_values"));
        assert!(helper.contains("set-cookie"));
        assert!(helper.contains("schema_type_hint"));
        assert!(helper.contains("resolve_response_ref"));
    }
}
