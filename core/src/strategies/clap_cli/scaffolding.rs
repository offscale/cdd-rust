#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Scaffolding for Clap CLI
//!
//! Generates CLI command signatures and imports.

use crate::openapi::parse::models::ParsedLink;
use crate::openapi::parse::models::ResponseHeader;

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
pub fn handler_signature(
    func_name: &str,
    args: &[String],
    response_type: Option<&str>,
    response_headers: &[ResponseHeader],
    _response_links: Option<&[ParsedLink]>,
) -> String {
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
        "#[derive(Args, Debug, Clone)]
pub struct {} {{
",
        struct_name
    );
    for arg in args {
        // arg might be `id: String`. We add `#[clap(long)]` to it.
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

    let mut body = String::new();
    body.push_str(
        "    // TODO: implement request logic using reqwest
",
    );
    body.push_str(
        "    todo!()
",
    );

    let func_def = format!(
        "pub async fn {}(args: {}, client: &Client, base_url: &str) -> {} {{
{}
}}
",
        func_name, struct_name, return_type, body
    );

    format!("{}{}", struct_def, func_def)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_imports() {
        let imports = handler_imports();
        assert!(imports.contains("use clap::{Args, Subcommand};"));
        assert!(imports.contains("use reqwest::Client;"));
    }

    #[test]
    fn test_handler_signature() {
        let sig = handler_signature(
            "create_user",
            &["id: String".to_string()],
            Some("User"),
            &[],
            None,
        );
        assert!(sig.contains("pub struct CreateUserArgs {"));
        assert!(sig.contains("#[clap(long)]"));
        assert!(sig.contains("pub id: String,"));
        assert!(sig.contains("pub async fn create_user(args: CreateUserArgs, client: &Client, base_url: &str) -> Result<User, reqwest::Error>"));
    }
}
