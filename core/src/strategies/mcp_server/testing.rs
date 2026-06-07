#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Test Generation for MCP Server
//!
//! Generates tests for SSE integration endpoints.

use crate::openapi::parse::RequestBodyDefinition;

/// Returns standard imports for MCP Server tests.
pub fn test_imports() -> String {
    let mut imports = String::new();
    imports.push_str("use actix_web::{test, App};\n");
    imports.push_str("use serde_json::json;\n");
    imports
}

/// Returns the test function signature.
pub fn test_fn_signature(fn_name: &str) -> String {
    format!("#[actix_web::test]\nasync fn test_{}() {{\n", fn_name)
}

/// Returns code to initialize the MCP server for testing.
pub fn test_app_init(app_factory: &str) -> String {
    format!(
        "    let mut app = test::init_service({}(App::new())).await;\n",
        app_factory
    )
}

/// Returns the body setup code.
pub fn test_body_setup_code(_body: &RequestBodyDefinition) -> String {
    "    let _req_body = json!({});\n".to_string()
}

/// Returns the request builder string.
pub fn test_request_builder(_method: &str, uri: &str, _body_setup: &str) -> String {
    // Generate dummy MCP JSON-RPC call request
    let mut code = String::new();
    code.push_str(
        "    let mcp_req = json!({\n        \"jsonrpc\": \"2.0\",\n        \"id\": 1,\n        \"method\": \"tools/call\",\n        \"params\": {\n            \"name\": \"dummy_tool\",\n            \"arguments\": {}\n        }\n    });\n"
    );
    code.push_str(&format!(
        "    let req = test::TestRequest::post().uri(\"{}\").insert_header((\"Authorization\", \"Bearer test_token\")).set_json(&mcp_req).to_request();\n",
        uri
    ));
    code
}

/// Returns the API call code.
pub fn test_api_call() -> String {
    "    let resp = test::call_service(&mut app, req).await;\n".to_string()
}

/// Returns assertion code.
pub fn test_assertion() -> String {
    "    assert!(resp.status().is_success());\n".to_string()
}

/// Returns the validation helper.
pub fn test_validation_helper() -> String {
    "".to_string()
}
