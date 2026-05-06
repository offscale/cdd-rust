#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Scaffolding
//!
//! Logic for generating Axum handler signatures, imports, and function bodies.

use crate::openapi::parse::models::ParsedLink;
use crate::openapi::parse::models::ResponseHeader;

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
pub fn handler_signature(
    func_name: &str,
    args: &[String],
    response_type: Option<&str>,
    response_headers: &[ResponseHeader],
    response_links: Option<&[ParsedLink]>,
) -> String {
    let args_str = args.join(", ");

    let has_headers = !response_headers.is_empty();
    let _has_links = response_links.map(|l| !l.is_empty()).unwrap_or(false);

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
                "    headers.insert(\"{}\", \"TODO\".parse().expect(\"invalid header value\"));\n",
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

    #[test]
    fn test_handler_imports() {
        assert!(handler_imports().contains("use axum::"));
    }

    #[test]
    fn test_handler_signature() {
        let sig = handler_signature(
            "my_handler",
            &["id: Path<i32>".to_string()],
            None,
            &[],
            None,
        );
        assert!(sig.contains("pub async fn my_handler(id: Path<i32>) -> impl IntoResponse"));
    }
}
