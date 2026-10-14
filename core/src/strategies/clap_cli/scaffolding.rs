#![deny(missing_docs)]

//! # Scaffolding for Clap CLI
//!
//! Generates CLI command `clap::Args` structs and `reqwest` HTTP execution functions.

use crate::openapi::parse::routes::naming::to_snake_case;
use crate::openapi::parse::{BodyFormat, ParamSource, ParsedRoute};

/// Returns standard imports for Clap CLI files.
///
/// # Returns
/// The Rust `use` statements required by generated CLI command modules.
pub fn handler_imports() -> String {
    let mut imports = String::new();
    imports.push_str(
        "use clap::{Args, Subcommand};
",
    );
    imports.push_str(
        "use reqwest::Client;
",
    );
    imports.push_str(
        "use serde::Deserialize;
",
    );
    imports.push_str(
        "use serde_json::Value;
",
    );
    imports.push_str(
        "use uuid::Uuid;
",
    );
    imports.push_str(
        "use chrono::{DateTime, Utc, NaiveDate, NaiveDateTime};
",
    );
    imports
}

/// Generates the Clap `Args` struct and the async CLI execution function that builds
/// and dispatches a `reqwest` HTTP call for the given [`ParsedRoute`].
///
/// # Arguments
/// * `route` - The parsed OpenAPI route definition.
/// * `args` - Extracted `name: Type` parameter declarations.
///
/// # Returns
/// The generated Rust struct and async function code.
pub fn handler_signature(route: &ParsedRoute, args: &[String]) -> String {
    let func_name = &route.handler_name;
    let response_headers = &route.response_headers;
    let response_type = route.response_type.as_deref();

    let struct_name = func_name
        .split('_')
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<String>()
        + "Args";

    let mut struct_def = format!(
        "#[derive(Args, Debug, Clone, Default)]
pub struct {} {{
",
        struct_name
    );
    for arg in args {
        struct_def.push_str(
            "    #[clap(long)]
",
        );
        struct_def.push_str(&format!(
            "    pub {},
",
            arg
        ));
    }
    struct_def.push_str(
        "}

",
    );

    let return_type = if !response_headers.is_empty() {
        "Result<reqwest::Response, reqwest::Error>".to_string()
    } else if let Some(rt) = response_type {
        format!("Result<{}, reqwest::Error>", rt)
    } else {
        "Result<reqwest::Response, reqwest::Error>".to_string()
    };

    let mut format_path = route.path.clone();
    let mut format_args = vec!["base_url.trim_end_matches('/')".to_string()];

    for param in route
        .params
        .iter()
        .chain(route.path_params.iter())
        .filter(|p| p.source == ParamSource::Path)
    {
        let placeholder = format!("{{{}}}", param.name);
        if format_path.contains(&placeholder) {
            format_path = format_path.replace(&placeholder, "{}");
            format_args.push(format!("args.{}", to_snake_case(&param.name)));
        }
    }

    let method_lower = route.method.to_lowercase();
    let mut body = String::new();
    body.push_str(&format!(
        r#"    let url = format!("{{}}{}", {});"#,
        format_path,
        format_args.join(", ")
    ));
    body.push(char::from(10u8));

    if matches!(
        method_lower.as_str(),
        "get" | "post" | "put" | "delete" | "patch" | "head"
    ) {
        body.push_str(&format!(
            "    let mut req = client.{}(&url);
",
            method_lower
        ));
    } else {
        body.push_str(&format!(
            r#"    let mut req = client.request(reqwest::Method::from_bytes(b"{}").unwrap_or(reqwest::Method::GET), &url);"#,
            route.method.to_uppercase()
        ));
        body.push(char::from(10u8));
    }

    let has_query = route
        .params
        .iter()
        .any(|p| p.source == ParamSource::Query || p.source == ParamSource::QueryString);
    if has_query && args.iter().any(|a| a.starts_with("query:")) {
        body.push_str(
            "    req = req.query(&args.query);
",
        );
    }

    for param in route
        .params
        .iter()
        .filter(|p| p.source == ParamSource::Header)
    {
        let field_name = to_snake_case(&param.name.replace('-', "_"));
        if args
            .iter()
            .any(|a| a.starts_with(&format!("{}:", field_name)))
        {
            body.push_str(&format!(
                r#"    req = req.header("{}", args.{}.to_string());"#,
                param.name, field_name
            ));
            body.push(char::from(10u8));
        }
    }

    if args.iter().any(|a| a.starts_with("auth_token:")) {
        body.push_str(
            "    if let Some(ref token) = args.auth_token {
",
        );
        body.push_str(
            "        req = req.bearer_auth(token);
",
        );
        body.push_str(
            "    }
",
        );
    }

    if let Some(req_body) = &route.request_body {
        if args.iter().any(|a| a.starts_with("body:")) {
            if req_body.required {
                match req_body.format {
                    BodyFormat::Json => body.push_str(
                        "    req = req.json(&args.body);
",
                    ),
                    BodyFormat::Form => body.push_str(
                        "    req = req.form(&args.body);
",
                    ),
                    _ => body.push_str(
                        "    req = req.body(args.body);
",
                    ),
                }
            } else {
                body.push_str(
                    "    if let Some(ref payload) = args.body {
",
                );
                match req_body.format {
                    BodyFormat::Json => body.push_str(
                        "        req = req.json(payload);
",
                    ),
                    BodyFormat::Form => body.push_str(
                        "        req = req.form(payload);
",
                    ),
                    _ => body.push_str(
                        "        req = req.body(payload.clone());
",
                    ),
                }
                body.push_str(
                    "    }
",
                );
            }
        }
    }

    body.push_str(
        "    let resp = req.send().await?.error_for_status()?;
",
    );
    if !response_headers.is_empty() || response_type.is_none() {
        body.push_str("    Ok(resp)");
    } else if let Some(rt) = response_type {
        body.push_str(&format!("    resp.json::<{}>().await", rt));
    }

    let func_def = format!(
        "/// @OAS_METHOD: {}
/// @OAS_PATH: {}
pub async fn {}(args: {}, client: &Client, base_url: &str) -> {} {{
{}
}}
",
        route.method.to_uppercase(),
        route.path,
        func_name,
        struct_name,
        return_type,
        body
    );

    format!("{}{}", struct_def, func_def)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::{
        RequestBodyDefinition, ResponseHeader, RouteKind, RouteParam,
    };
    use ra_ap_edition::Edition;
    use ra_ap_syntax::SourceFile;
    use std::collections::BTreeMap;

    fn dummy_route(
        handler_name: &str,
        response_type: Option<&str>,
    ) -> crate::openapi::parse::ParsedRoute {
        crate::openapi::parse::ParsedRoute {
            path: "/users/{id}".into(),
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
            method: "POST".into(),
            handler_name: handler_name.into(),
            operation_id: None,
            params: vec![
                RouteParam {
                    name: "id".to_string(),
                    source: ParamSource::Path,
                    ty: "String".to_string(),
                    description: None,
                    example: None,
                    style: None,
                    explode: false,
                    allow_empty_value: false,
                    allow_reserved: false,
                    content_media_type: None,
                    raw_schema: None,
                    deprecated: false,
                    extensions: BTreeMap::new(),
                },
                RouteParam {
                    name: "filter".to_string(),
                    source: ParamSource::Query,
                    ty: "String".to_string(),
                    description: None,
                    example: None,
                    style: None,
                    explode: false,
                    allow_empty_value: false,
                    allow_reserved: false,
                    content_media_type: None,
                    raw_schema: None,
                    deprecated: false,
                    extensions: BTreeMap::new(),
                },
                RouteParam {
                    name: "x-api-key".to_string(),
                    source: ParamSource::Header,
                    ty: "String".to_string(),
                    description: None,
                    example: None,
                    style: None,
                    explode: false,
                    allow_empty_value: false,
                    allow_reserved: false,
                    content_media_type: None,
                    raw_schema: None,
                    deprecated: false,
                    extensions: BTreeMap::new(),
                },
            ],
            path_params: vec![],
            request_body: Some(RequestBodyDefinition {
                ty: "Value".to_string(),
                description: None,
                media_type: "application/json".to_string(),
                format: BodyFormat::Json,
                required: true,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
                example: None,
            }),
            security: vec![],
            security_defined: false,
            response_type: response_type.map(|s| s.to_string()),
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
        let imports = handler_imports();
        assert!(imports.contains("use clap::{Args, Subcommand};"));
        assert!(imports.contains("use reqwest::Client;"));
    }

    #[test]
    fn test_handler_signature_generates_real_reqwest_call() {
        let route = dummy_route("create_user", Some("User"));
        let sig = handler_signature(
            &route,
            &[
                "id: String".to_string(),
                "query: Value".to_string(),
                "x_api_key: String".to_string(),
                "auth_token: Option<String>".to_string(),
                "body: Value".to_string(),
            ],
        );
        let parse = std::mem::ManuallyDrop::new(SourceFile::parse(&sig, Edition::Edition2021));
        assert!(
            parse.errors().is_empty(),
            "AST errors: {:?}",
            parse.errors()
        );
        assert!(sig.contains("pub struct CreateUserArgs {"));
        assert!(sig.contains("#[clap(long)]"));
        assert!(sig.contains("pub id: String,"));
        assert!(sig.contains("pub async fn create_user(args: CreateUserArgs, client: &Client, base_url: &str) -> Result<User, reqwest::Error>"));
        assert!(sig.contains("client.post(&url)"));
        assert!(sig.contains("req = req.query(&args.query);"));
        assert!(sig.contains("args.x_api_key.to_string()"));
        assert!(sig.contains("req = req.bearer_auth(token);"));
        assert!(sig.contains("req = req.json(&args.body);"));
        assert!(sig.contains("resp.json::<User>().await"));

        let mut custom_route = dummy_route("custom_op", None);
        custom_route.method = "CUSTOM".to_string();
        custom_route.response_headers = vec![ResponseHeader {
            name: "X-Rate".to_string(),
            ty: "String".to_string(),
            required: false,
            description: None,
            example: None,
            style: None,
            explode: None,
            content_media_type: None,
            deprecated: false,
            extensions: BTreeMap::new(),
        }];
        if let Some(ref mut rb) = custom_route.request_body {
            rb.required = false;
            rb.format = BodyFormat::Form;
        }
        let sig_custom = handler_signature(
            &custom_route,
            &["id: String".to_string(), "body: Option<Value>".to_string()],
        );
        let parse_custom =
            std::mem::ManuallyDrop::new(SourceFile::parse(&sig_custom, Edition::Edition2021));
        assert!(parse_custom.errors().is_empty());
        assert!(sig_custom.contains("Method::from_bytes"));
        assert!(sig_custom.contains("req = req.form(payload);"));
    }
}
