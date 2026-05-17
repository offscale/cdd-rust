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

    if let Some(bp) = &route.base_path {
        format_str.push_str(bp);
    }

    let mut chars = route.path.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut param_name = String::new();
            while let Some(&p) = chars.peek() {
                if p == '}' {
                    chars.next();
                    break;
                }
                param_name.push(chars.next().unwrap_or('_'));
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
    if args.iter().any(|a| a.starts_with("query:")) {
        body.push_str("    let qs = serde_qs::Config::new().array_format(serde_qs::ArrayFormat::Unindexed).serialize_string(&query).unwrap_or_default();\n");
        body.push_str("    let url = if url.contains('?') { format!(\"{}&{}\", url, qs) } else { format!(\"{}?{}\", url, qs) };\n");
        body.push_str(&format!(
            "    let mut req = client.request(reqwest::Method::from_bytes(b\"{}\").expect(\"valid method\"), url);\n",
            method
        ));
    } else {
        body.push_str(&format!(
            "    let mut req = client.request(reqwest::Method::from_bytes(b\"{}\").expect(\"valid method\"), url);\n",
            method
        ));
    }

    for param in &route.params {
        let var_name = crate::openapi::parse::routes::naming::to_snake_case(&param.name);
        match param.source {
            crate::openapi::parse::ParamSource::Header => {
                body.push_str(&format!(
                    "    req = req.header(\"{}\", &{});\n",
                    param.name, var_name
                ));
            }
            crate::openapi::parse::ParamSource::Cookie => {
                body.push_str(&format!(
                    "    req = req.header(\"Cookie\", format!(\"{}={{}}\", {}));\n",
                    param.name, var_name
                ));
            }
            _ => {}
        }
    }

    let mut has_security = false;
    let mut auth_code = String::new();
    for group in &route.security {
        if group.is_anonymous() {
            continue;
        }
        has_security = true;
        for req in &group.schemes {
            if let Some(info) = &req.scheme {
                match &info.kind {
                    crate::openapi::parse::models::SecuritySchemeKind::ApiKey { name, in_loc } => {
                        if in_loc == &crate::openapi::parse::ParamSource::Header {
                            auth_code.push_str(&format!(
                                "        req = req.header(\"{}\", token);\n",
                                name
                            ));
                        } else if in_loc == &crate::openapi::parse::ParamSource::Query {
                            auth_code.push_str(&format!(
                                "        req = req.query(&[(\"{}\", token)]);\n",
                                name
                            ));
                        }
                    }
                    crate::openapi::parse::models::SecuritySchemeKind::OAuth2 { .. }
                    | crate::openapi::parse::models::SecuritySchemeKind::Http { .. }
                    | crate::openapi::parse::models::SecuritySchemeKind::OpenIdConnect { .. } => {
                        auth_code.push_str("        req = req.bearer_auth(token);\n");
                    }
                    _ => {}
                }
            } else {
                auth_code.push_str("        req = req.bearer_auth(token);\n");
            }
        }
        break;
    }

    if has_security {
        body.push_str("    if let Some(token) = auth_token {\n");
        body.push_str(&auth_code);
        body.push_str("    }\n");
    }

    if let Some(body_arg) = args.iter().find(|a| a.starts_with("body:")) {
        let is_form = route
            .request_body
            .as_ref()
            .map(|b| b.format == crate::openapi::parse::models::BodyFormat::Form)
            .unwrap_or(false);
        let is_multipart = route
            .request_body
            .as_ref()
            .map(|b| b.format == crate::openapi::parse::models::BodyFormat::Multipart)
            .unwrap_or(false);

        let method = if is_multipart {
            "multipart"
        } else if is_form {
            "form"
        } else {
            "json"
        };

        if body_arg.contains("Option<") {
            body.push_str(&format!(
                "    if let Some(b) = body {{ req = req.{}(&b); }}\n",
                method
            ));
        } else {
            body.push_str(&format!("    req = req.{}(&body);\n", method));
        }
    }

    body.push_str("    let resp = req.send().await?.error_for_status()?;\n");

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
