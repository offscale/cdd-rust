#![deny(missing_docs)]

//! # Test Generation for MCP Server
//!
//! Generates tests for SSE and JSON-RPC 2.0 MCP Server endpoints.

use crate::openapi::parse::RequestBodyDefinition;

/// Returns standard imports for MCP Server tests.
///
/// # Returns
/// The Rust `use` statements required for MCP Server tests.
pub fn test_imports() -> String {
    let mut imports = String::new();
    imports.push_str(
        "use actix_web::{test, App};
",
    );
    imports.push_str(
        "use cdd_core::mcp::models::{CallToolRequestParams, JsonRpcRequest, JsonRpcResponse, RequestId};
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
/// The `#[actix_web::test]` function header.
pub fn test_fn_signature(fn_name: &str) -> String {
    format!(
        "#[actix_web::test]
async fn test_{}() {{
",
        fn_name
    )
}

/// Returns code to initialize the MCP server for testing.
///
/// # Arguments
/// * `app_factory` - Path to the service configuration factory.
///
/// # Returns
/// Rust statement initializing `app`.
pub fn test_app_init(app_factory: &str) -> String {
    format!(
        "    let mut app = test::init_service({}(App::new())).await;
",
        app_factory
    )
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
/// * `uri` - Target URI.
/// * `body_setup` - Optional body setup statements.
///
/// # Returns
/// Rust statements constructing the Actix `TestRequest`.
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
        r#"    let mcp_req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: RequestId::Number(1),
        method: "tools/call".to_string(),
        params: Some(serde_json::to_value(&call_params).expect("serialize params")),
    };
"#,
    );
    code.push_str(&format!(
        r#"    let req = test::TestRequest::post().uri("{}").insert_header(("Authorization", "Bearer test_token")).set_json(&mcp_req).to_request();"#,
        uri
    ));
    code.push(char::from(10u8));
    code
}

/// Returns the API call code.
///
/// # Returns
/// Rust statement calling `test::call_service`.
pub fn test_api_call() -> String {
    "    let resp = test::call_service(&mut app, req).await;
"
    .to_string()
}

/// Returns assertion code verifying the HTTP response status.
///
/// # Returns
/// Rust assertion statements.
pub fn test_assertion() -> String {
    "    assert!(resp.status().is_success());
"
    .to_string()
}

/// Returns the SSE and JSON-RPC 2.0 validation helper.
///
/// # Returns
/// Helper function `validate_mcp_sse_frame`.
pub fn test_validation_helper() -> String {
    r#"fn validate_mcp_sse_frame(frame: &str) -> JsonRpcResponse {
    assert!(
        frame.starts_with("event: message"),
        "Expected SSE message frame, got: {}",
        frame
    );
    let data_line = frame
        .lines()
        .find_map(|l| l.strip_prefix("data: "))
        .expect("Missing SSE data line");
    let rpc_resp: JsonRpcResponse =
        serde_json::from_str(data_line).expect("Invalid JSON-RPC response in SSE frame");
    assert_eq!(rpc_resp.jsonrpc, "2.0");
    assert!(rpc_resp.error.is_none(), "Unexpected JSON-RPC error: {:?}", rpc_resp.error);
    rpc_resp
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
    fn test_mcp_server_testing_ast() {
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
            test_fn_signature("mcp_sse"),
            test_app_init("config"),
            test_request_builder("POST", "/pets", &setup),
            test_api_call(),
            test_assertion()
        );
        let parse = std::mem::ManuallyDrop::new(SourceFile::parse(&full, Edition::Edition2021));
        assert!(
            parse.errors().is_empty(),
            "Generated invalid Rust AST: {:?}
Code:
{}",
            parse.errors(),
            full
        );
        assert!(full.contains("validate_mcp_sse_frame"));
        assert!(full.contains("CallToolRequestParams"));
    }
}
