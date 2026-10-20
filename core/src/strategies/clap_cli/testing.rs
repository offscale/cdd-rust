#![deny(missing_docs)]

//! # Test Generation for Clap CLI
//!
//! Generates CLI argument parsing and mock-server command execution tests.

use crate::openapi::parse::routes::naming::to_snake_case;
use crate::openapi::parse::{ParamSource, ParsedRoute, RequestBodyDefinition};

/// Returns standard imports for Clap CLI tests.
///
/// # Returns
/// The Rust `use` statements required for CLI tests.
pub fn test_imports() -> String {
    let mut imports = String::new();
    imports.push_str(
        "use clap::Parser;
",
    );
    imports.push_str(
        "use reqwest::Client;
",
    );
    imports.push_str(
        "use serde_json::json;
",
    );
    imports
}

/// Returns the test function signature.
///
/// # Arguments
/// * `fn_name` - Name of the test function (without `test_` prefix).
///
/// # Returns
/// The `#[tokio::test]` function header.
pub fn test_fn_signature(fn_name: &str) -> String {
    format!(
        "#[tokio::test]
async fn test_{}() {{
",
        fn_name
    )
}

/// Returns code to initialize the HTTP client and base URL for CLI testing.
///
/// # Arguments
/// * `app_factory` - Unused factory name.
///
/// # Returns
/// Rust statements initializing `client` and `base_url`.
pub fn test_app_init(app_factory: &str) -> String {
    let _ = app_factory;
    let mut s = String::from(
        "    let client = Client::new();
",
    );
    s.push_str(r#"    let base_url = "http://localhost:8080";"#);
    s.push(char::from(10u8));
    s
}

/// Returns the body setup code.
///
/// # Arguments
/// * `_body` - Request body definition.
///
/// # Returns
/// Rust statement defining `req_body`.
pub fn test_body_setup_code(_body: &RequestBodyDefinition) -> String {
    "    let req_body = json!({});
"
    .to_string()
}

/// Returns the request builder string.
///
/// # Arguments
/// * `method` - HTTP method string.
/// * `uri` - Target request URI.
/// * `body_setup` - Optional body setup statements.
///
/// # Returns
/// Rust statements constructing the `reqwest` call.
pub fn test_request_builder(method: &str, uri: &str, body_setup: &str) -> String {
    let mut code = String::new();
    if !body_setup.is_empty() {
        code.push_str(body_setup);
    }
    let lower_method = method.to_lowercase();
    code.push_str(&format!(
        r#"    let req = client.{}(format!("{{}}{{}}", base_url, "{}"))"#,
        lower_method, uri
    ));
    if !body_setup.is_empty() {
        code.push_str(".json(&req_body)");
    }
    code.push_str(
        ";
",
    );
    code
}

/// Returns the API call code.
///
/// # Returns
/// Rust statement sending the request.
pub fn test_api_call() -> String {
    let mut s =
        String::from(r#"    let resp = req.send().await.expect("Failed to send request");"#);
    s.push(char::from(10u8));
    s
}

/// Returns assertion code.
///
/// # Returns
/// Rust statement asserting a successful HTTP status.
pub fn test_assertion() -> String {
    "    assert!(resp.status().is_success());
"
    .to_string()
}

/// Returns the CLI argument validation helper.
///
/// # Returns
/// Helper function verifying CLI argument parsing and command execution.
pub fn test_validation_helper() -> String {
    r#"fn parse_cli_from<T: clap::FromArgMatches>(
    cmd: clap::Command,
    argv: &[&str],
) -> Result<T, clap::Error> {
    let matches = cmd.try_get_matches_from(argv)?;
    T::from_arg_matches(&matches)
}
"#
    .to_string()
}

/// Generates a custom CLI test that constructs the Clap `Args` struct via CLI flags
/// (`clap::Command` + `FromArgMatches`) and invokes the generated CLI command handler.
///
/// # Arguments
/// * `route` - The [`ParsedRoute`] under test.
/// * `app_factory` - Crate/module prefix for the generated CLI package.
///
/// # Returns
/// Complete `#[tokio::test]` function string.
pub fn generate_custom_test(route: &ParsedRoute, app_factory: &str) -> String {
    let fn_name = format!("test_cli_{}", to_snake_case(&route.handler_name));
    let tag = route
        .tags
        .first()
        .cloned()
        .unwrap_or_else(|| "default".to_string());
    let module_name = to_snake_case(&tag);
    let handler_name = &route.handler_name;
    let crate_name = if app_factory.is_empty() || app_factory.contains("::") {
        "crate"
    } else {
        app_factory
    };

    let struct_name = handler_name
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

    let mut cli_flags = vec![r#""cli""#.to_string()];
    for param in route
        .params
        .iter()
        .filter(|p| p.source == ParamSource::Path || p.source == ParamSource::Header)
    {
        let flag_name = to_snake_case(&param.name).replace('_', "-");
        let parsed_ty = crate::openapi::parse::models::RustType::parse(&param.ty);
        let sample = if parsed_ty.is_integer() {
            "1"
        } else if parsed_ty.is_bool() {
            "true"
        } else if parsed_ty.is_uuid() {
            "00000000-0000-0000-0000-000000000000"
        } else {
            "test_val"
        };
        cli_flags.push(format!(r#""--{}""#, flag_name));
        cli_flags.push(format!(r#""{}""#, sample));
    }

    let mut code = String::new();
    code.push_str(&format!(
        "#[tokio::test]
async fn {}() {{
",
        fn_name
    ));
    code.push_str(
        "    use clap::{Command, Args as ClapArgs, FromArgMatches};
",
    );
    code.push_str(
        "    let client = reqwest::Client::new();
",
    );
    code.push_str(r#"    let base_url = "http://localhost:8080";"#);
    code.push(char::from(10u8));
    code.push_str(&format!(
        r#"    let cmd = {}::handlers::{}::{}::augment_args(Command::new("cli"));"#,
        crate_name, module_name, struct_name
    ));
    code.push(char::from(10u8));
    code.push_str(&format!(
        r#"    let matches = cmd.try_get_matches_from([{}]).expect("valid CLI flags");"#,
        cli_flags.join(", ")
    ));
    code.push(char::from(10u8));
    code.push_str(&format!(
        r#"    let args = {}::handlers::{}::{}::from_arg_matches(&matches).expect("parsed args");"#,
        crate_name, module_name, struct_name
    ));
    code.push(char::from(10u8));
    code.push_str(&format!(
        "    let result = {}::handlers::{}::{}(args, &client, base_url).await;
",
        crate_name, module_name, handler_name
    ));
    code.push_str(
        r#"    assert!(result.is_ok(), "expected CLI command execution to succeed, got {:?}", result.err());"#,
    );
    code.push_str(
        "
}
",
    );
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::{BodyFormat, RouteKind, RouteParam};
    use ra_ap_edition::Edition;
    use ra_ap_syntax::SourceFile;
    use std::collections::BTreeMap;

    fn assert_valid_rust_syntax(code: &str) {
        let parse = SourceFile::parse(code, Edition::Edition2021);
        assert!(
            parse.errors().is_empty(),
            "Generated invalid Rust AST: {:?}
Code:
{}",
            parse.errors(),
            code
        );
    }

    #[test]
    fn test_clap_cli_testing_components_and_custom_test_ast() {
        let body = RequestBodyDefinition {
            ty: "Value".to_string(),
            description: None,
            media_type: "application/json".to_string(),
            format: BodyFormat::Json,
            required: true,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
            example: None,
        };
        let setup = test_body_setup_code(&body);
        let full = format!(
            "{}
{}
{}
{}{}{}{}
}}",
            test_imports(),
            test_validation_helper(),
            test_fn_signature("cli_op"),
            test_app_init("app"),
            test_request_builder("POST", "/users", &setup),
            test_api_call(),
            test_assertion()
        );
        assert_valid_rust_syntax(&full);

        let route = ParsedRoute {
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
            method: "GET".into(),
            handler_name: "get_user".into(),
            operation_id: None,
            params: vec![
                RouteParam {
                    name: "id".to_string(),
                    source: ParamSource::Path,
                    ty: "i64".to_string(),
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
                    name: "x_trace".to_string(),
                    source: ParamSource::Header,
                    ty: "Uuid".to_string(),
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
            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: Some("Value".to_string()),
            response_status: Some("200".to_string()),
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec!["users".to_string()],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let custom = generate_custom_test(&route, "my_cli");
        assert_valid_rust_syntax(&custom);
        assert!(custom.contains("augment_args"));
        assert!(custom.contains("--id"));
        assert!(custom.contains("00000000-0000-0000-0000-000000000000"));

        let empty_req = test_request_builder("GET", "/users", "");
        assert!(!empty_req.contains(".json("));

        let mut route2 = route.clone();
        route2.handler_name = "do__work".to_string();
        route2.tags.clear();
        route2.params = vec![
            RouteParam {
                name: "flag".to_string(),
                source: ParamSource::Path,
                ty: "bool".to_string(),
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
                name: "name".to_string(),
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
            RouteParam {
                name: "ignored".to_string(),
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
        ];
        let custom2 = generate_custom_test(&route2, "crate::create_app");
        assert_valid_rust_syntax(&custom2);
        assert!(custom2.contains("crate::handlers::default::DoWorkArgs"));
        assert!(custom2.contains("\"true\""));
        assert!(custom2.contains("\"test_val\""));

        let custom3 = generate_custom_test(&route2, "");
        assert!(custom3.contains("crate::handlers::default::DoWorkArgs"));
    }
}
