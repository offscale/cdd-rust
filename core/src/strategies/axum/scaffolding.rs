#![deny(missing_docs)]

//! # Scaffolding
//!
//! Logic for generating Axum handler signatures, imports, and function bodies.

/// Returns standard imports for Axum handlers.
pub fn handler_imports() -> String {
    let mut imports = String::new();
    imports.push_str("use axum::{extract::{Path, Query, Json, Form, Multipart}, response::IntoResponse, http::{StatusCode, HeaderMap}};\n");
    imports.push_str("use serde::Deserialize;\n");
    imports.push_str("use serde_json::Value;\n");
    imports.push_str("use uuid::Uuid;\n");
    imports.push_str("use chrono::{DateTime, Utc, NaiveDate, NaiveDateTime};\n");
    imports
}

/// Generates the handler function signature and body scaffold.
///
/// # Arguments
/// * `route` - Input `route` parameter.
/// * `args` - Input `args` parameter.
///
/// # Returns
/// Resulting `String` value.
pub fn handler_signature(route: &crate::openapi::parse::ParsedRoute, args: &[String]) -> String {
    let func_name = &route.handler_name;
    let response_headers = &route.response_headers;
    let response_type = route.response_type.as_deref();

    let args_str = args.join(", ");

    let has_headers = !response_headers.is_empty();

    let return_type = if has_headers {
        "impl IntoResponse".to_string()
    } else if let Some(rt) = response_type {
        format!("Json<{}>", rt)
    } else {
        "impl IntoResponse".to_string()
    };

    let mut body = String::new();

    // Body content logic
    body.push_str("    // TODO: Implement logic\n");

    if has_headers {
        body.push_str("    let mut headers = HeaderMap::new();\n");
        for header in response_headers {
            body.push_str(&format!(
                "    headers.insert(\"{}\", axum::http::HeaderValue::from_static(\"TODO\"));\n",
                header.name
            ));
        }
        if let Some(rt) = response_type {
            body.push_str(&format!("    let response = {} {{}};\n", rt));
            body.push_str("    (StatusCode::OK, headers, Json(response)).into_response()\n");
        } else {
            body.push_str("    (StatusCode::OK, headers).into_response()\n");
        }
    } else if let Some(rt) = response_type {
        body.push_str(&format!("    let response = {} {{}};\n", rt));
        body.push_str("    Json(response)\n");
    } else {
        body.push_str("    StatusCode::OK\n");
    }

    format!(
        "pub async fn {}({}) -> {} {{\n{}\n}}",
        func_name, args_str, return_type, body
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::RouteKind;
    use std::collections::BTreeMap;

    fn dummy_route() -> crate::openapi::parse::ParsedRoute {
        crate::openapi::parse::ParsedRoute {
            path: "/".into(),
            summary: None,
            description: None,
            path_summary: None,
            path_description: None,
            path_extensions: BTreeMap::new(),
            operation_summary: None,
            operation_description: None,
            base_path: None,
            path_servers: None,
            servers_override: None,
            method: "GET".into(),
            handler_name: "my_handler".into(),
            operation_id: None,
            params: vec![],
            path_params: vec![],
            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        }
    }

    #[test]
    fn test_handler_imports() {
        assert!(handler_imports().contains("use axum::"));
    }

    #[test]
    fn test_handler_signature() {
        let mut route = dummy_route();
        let sig = handler_signature(&route, &["id: Path<i32>".to_string()]);
        assert!(sig.contains("pub async fn my_handler(id: Path<i32>) -> impl IntoResponse"));

        route.response_type = Some("User".to_string());
        let sig_typed = handler_signature(&route, &[]);
        assert!(sig_typed.contains("-> Json<User>"));

        route.response_headers = vec![crate::openapi::parse::models::ResponseHeader {
            name: "X-Rate".to_string(),
            description: None,
            required: false,
            deprecated: false,
            style: None,
            explode: None,
            ty: "String".to_string(),
            content_media_type: None,
            example: None,
            extensions: BTreeMap::new(),
        }];
        let sig_hdr_typed = handler_signature(&route, &[]);
        assert!(sig_hdr_typed.contains("(StatusCode::OK, headers, Json(response)).into_response()"));

        route.response_type = None;
        let sig_hdr_only = handler_signature(&route, &[]);
        assert!(sig_hdr_only.contains("(StatusCode::OK, headers).into_response()"));
    }
}
