#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Scaffolding for Reqwest Client
//!
//! Generates client method signatures and imports.

/// Returns standard imports for Reqwest client files.
pub fn handler_imports() -> String {
    let mut imports = String::new();
    imports.push_str("use reqwest::Client;\n");
    imports.push_str("use serde::{Deserialize, Serialize};\n");
    imports.push_str("use serde_json::Value;\n");
    imports.push_str("use uuid::Uuid;\n");
    imports.push_str("use chrono::{DateTime, Utc, NaiveDate, NaiveDateTime};\n");
    imports.push_str("use crate::models::*;\n");
    imports
}

/// Generates the client function signature and body scaffold.
pub fn handler_signature(route: &crate::openapi::parse::ParsedRoute, args: &[String]) -> String {
    let func_name = &route.handler_name;
    let response_headers = &route.response_headers;
    let response_type = route.response_type.as_deref();

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
    let method = route.method.to_uppercase();

    // Convert OpenAPI path like /user/{username} to /user/{}
    let mut format_args = vec!["base_url".to_string()];
    let mut format_str = String::from("{}");

    let mut chars = route.path.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut param_name = String::new();
            while let Some(&p) = chars.peek() {
                if p == '}' {
                    chars.next();
                    break;
                }
                param_name.push(chars.next().unwrap());
            }
            format_str.push_str("{}");
            format_args.push(crate::openapi::parse::routes::naming::to_snake_case(
                &param_name,
            ));
        } else {
            format_str.push(c);
        }
    }

    body.push_str(&format!(
        "    let url = format!(\"{}\", {});\n",
        format_str,
        format_args.join(", ")
    ));
    body.push_str(&format!(
        "    let mut req = client.request(reqwest::Method::from_bytes(b\"{}\").unwrap(), url);\n",
        method
    ));

    if args.iter().any(|a| a.starts_with("query:")) {
        body.push_str("    req = req.query(&query);\n");
    }
    if args.iter().any(|a| a.starts_with("body:")) {
        if args.iter().any(|a| a.contains("Option<")) {
            body.push_str("    if let Some(b) = body { req = req.json(&b); }\n");
        } else {
            body.push_str("    req = req.json(&body);\n");
        }
    }

    body.push_str("    let resp = req.send().await?;\n");

    if !response_headers.is_empty() {
        body.push_str("    Ok(resp)\n");
    } else if let Some(rt) = response_type {
        body.push_str(&format!("    resp.json::<{}>().await\n", rt));
    } else {
        body.push_str("    Ok(resp)\n");
    }

    format!(
        "pub async fn {}({}) -> {} {{\n{}\n}}\n",
        func_name, args_str, return_type, body
    )
}
