#![deny(missing_docs)]

//! # Test Generation for MCP Client
//!
//! Generates tests for programmatic MCP tool definitions and `tools/call` adapters.

use crate::openapi::parse::RequestBodyDefinition;

/// Returns standard imports for MCP Client tests.
///
/// # Returns
/// The Rust `use` statements required for MCP client tests.
pub fn test_imports() -> String {
    let mut imports = String::new();
    imports.push_str(
        "use cdd_core::mcp::models::{CallToolRequestParams, CallToolResult, JsonRpcRequest, RequestId, Tool, ToolInputSchema};
",
    );
    imports.push_str(
        "use serde_json::json;
",
    );
    imports.push_str(
        "use std::sync::Arc;
",
    );
    imports.push_str(
        "use reqwest::Client;
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

/// Returns code to initialize the MCP client adapter for testing.
///
/// # Arguments
/// * `_app_factory` - Unused factory name.
///
/// # Returns
/// Rust statements initializing `client` and `base_url`.
pub fn test_app_init(_app_factory: &str) -> String {
    let mut s = String::from(
        "    let client = Arc::new(Client::new());
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

/// Returns the request builder string constructing a strongly typed [`crate::mcp::models::JsonRpcRequest`].
///
/// # Arguments
/// * `_method` - HTTP method string.
/// * `uri` - Target request URI or tool identifier.
/// * `body_setup` - Optional body setup statements.
///
/// # Returns
/// Rust statements constructing `rpc_req`.
pub fn test_request_builder(_method: &str, uri: &str, body_setup: &str) -> String {
    let mut code = String::new();
    if !body_setup.is_empty() {
        code.push_str(body_setup);
    } else {
        code.push_str(
            "    let req_body = json!({});
",
        );
    }
    let tool_name = uri.trim_start_matches('/').replace('/', "_");
    code.push_str(&format!(
        r#"    let call_params = CallToolRequestParams {{
        name: "{}".to_string(),
        arguments: Some(req_body),
    }};
"#,
        tool_name
    ));
    code.push_str(
        r#"    let rpc_req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: RequestId::Number(1),
        method: "tools/call".to_string(),
        params: Some(serde_json::to_value(&call_params).expect("serialize params")),
    };
"#,
    );
    code
}

/// Returns the API call code dispatching `rpc_req` over `client`.
///
/// # Returns
/// Rust statement executing `client.post(base_url).json(&rpc_req)`.
pub fn test_api_call() -> String {
    let mut s = String::from(
        r#"    let resp = client.post(base_url).json(&rpc_req).send().await.expect("Failed to send MCP request");"#,
    );
    s.push(char::from(10u8));
    s
}

/// Returns assertion code verifying the HTTP response and MCP [`crate::mcp::models::CallToolResult`].
///
/// # Returns
/// Rust assertion statements.
pub fn test_assertion() -> String {
    let mut s = String::from(
        "    assert!(resp.status().is_success());
",
    );
    s.push_str(
        r#"    let tool_res: CallToolResult = resp.json().await.expect("valid CallToolResult");"#,
    );
    s.push_str(
        "
    assert_ne!(tool_res.is_error, Some(true));
",
    );
    s
}

/// Returns the validation helper verifying [`crate::mcp::models::Tool`] metadata.
///
/// # Returns
/// Helper function `validate_mcp_tool`.
pub fn test_validation_helper() -> String {
    r#"fn validate_mcp_tool(tool: &Tool) {
    assert!(!tool.name.is_empty(), "MCP tool name must not be empty");
    assert_eq!(tool.input_schema.schema_type, "object");
}
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::BodyFormat;
    use ra_ap_edition::Edition;
    use ra_ap_syntax::SourceFile;

    #[test]
    fn test_mcp_client_testing_ast() {
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
            test_fn_signature("mcp_call"),
            test_app_init("app"),
            test_request_builder("POST", "/pets", &setup),
            test_api_call(),
            test_assertion()
        );
        let parse = SourceFile::parse(&full, Edition::Edition2021);
        assert!(
            parse.errors().is_empty(),
            "Generated invalid Rust AST: {:?}
Code:
{}",
            parse.errors(),
            full
        );
        assert!(full.contains("CallToolRequestParams"));
        assert!(full.contains("CallToolResult"));
        assert!(full.contains("validate_mcp_tool"));
    }
}
