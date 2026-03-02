#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Scaffolding for Reqwest Client
//!
//! Generates client method signatures and imports.

use crate::openapi::parse::models::ParsedLink;
use crate::openapi::parse::models::ResponseHeader;

/// Returns standard imports for Reqwest client files.
pub fn handler_imports() -> String {
    let mut imports = String::new();
    imports.push_str("use reqwest::Client;\n");
    imports.push_str("use serde::Deserialize;\n");
    imports.push_str("use serde_json::Value;\n");
    imports.push_str("use uuid::Uuid;\n");
    imports.push_str("use chrono::{DateTime, Utc, NaiveDate, NaiveDateTime};\n");
    imports
}

/// Generates the client function signature and body scaffold.
pub fn handler_signature(
    func_name: &str,
    args: &[String],
    response_type: Option<&str>,
    response_headers: &[ResponseHeader],
    _response_links: Option<&[ParsedLink]>,
) -> String {
    let mut all_args = vec!["client: &Client".to_string(), "base_url: &str".to_string()];
    all_args.extend_from_slice(args);
    let args_str = all_args.join(", ");

    let return_type = if !response_headers.is_empty() {
        "Result<reqwest::Response, reqwest::Error>".to_string()
    } else if let Some(rt) = response_type {
        format!("Result<{}, reqwest::Error>", rt)
    } else {
        "Result<reqwest::Response, reqwest::Error>".to_string()
    };

    let mut body = String::new();
    body.push_str("    // TODO: implement request logic\n");
    body.push_str("    todo!()\n");

    format!(
        "pub async fn {}({}) -> {} {{\n{}\n}}\n",
        func_name, args_str, return_type, body
    )
}
