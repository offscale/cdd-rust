#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Scaffolding for Clap CLI
//!
//! Generates CLI command signatures and imports.

/// Returns standard imports for Clap CLI files.
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

/// Generates the client function signature and body scaffold.
pub fn handler_signature(route: &crate::openapi::parse::ParsedRoute, args: &[String]) -> String {
    let func_name = &route.handler_name;
    let response_headers = &route.response_headers;
    let response_type = route.response_type.as_deref();

    // We generate a Clap Args struct, and a function that takes it.

    // Create the struct name from the function name (e.g. create_user -> CreateUserArgs)
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
        "#[derive(Args, Debug, Clone)]\npub struct {} {{\n",
        struct_name
    );
    for arg in args {
        // arg might be `id: String`. We add `#[clap(long)]` to it.
        struct_def.push_str("    #[clap(long)]\n");
        struct_def.push_str(&format!("    pub {},\n", arg));
    }
    struct_def.push_str("}\n\n");

    let return_type = if !response_headers.is_empty() {
        "Result<reqwest::Response, reqwest::Error>".to_string()
    } else if let Some(rt) = response_type {
        format!("Result<{}, reqwest::Error>", rt)
    } else {
        "Result<reqwest::Response, reqwest::Error>".to_string()
    };

    let mut body = String::new();
    body.push_str("    // TODO: implement request logic using reqwest\n");
    body.push_str("    todo!()\n");

    let func_def = format!(
        "pub async fn {}(args: {}, client: &Client, base_url: &str) -> {} {{\n{}\n}}\n",
        func_name, struct_name, return_type, body
    );

    format!("{}{}", struct_def, func_def)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::RouteKind;
    use std::collections::BTreeMap;

    fn dummy_route(
        handler_name: &str,
        response_type: Option<&str>,
    ) -> crate::openapi::parse::ParsedRoute {
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
            handler_name: handler_name.into(),
            operation_id: None,
            params: vec![],
            path_params: vec![],
            request_body: None,
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
    fn test_handler_signature() {
        let route = dummy_route("create_user", Some("User"));
        let sig = handler_signature(&route, &["id: String".to_string()]);
        assert!(sig.contains("pub struct CreateUserArgs {"));
        assert!(sig.contains("#[clap(long)]"));
        assert!(sig.contains("pub id: String,"));
        assert!(sig.contains("pub async fn create_user(args: CreateUserArgs, client: &Client, base_url: &str) -> Result<User, reqwest::Error>"));
    }
}
