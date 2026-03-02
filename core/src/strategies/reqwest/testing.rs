#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Test Generation for Reqwest Client
//!
//! Generates tests that use reqwest to call endpoints.

use crate::openapi::parse::RequestBodyDefinition;

/// Returns standard imports for client tests.
pub fn test_imports() -> String {
    let mut imports = String::new();
    imports.push_str("use reqwest::Client;\n");
    imports.push_str("use serde_json::json;\n");
    imports
}

/// Returns the test function signature.
pub fn test_fn_signature(fn_name: &str) -> String {
    format!("#[tokio::test]\nasync fn test_{}() {{\n", fn_name)
}

/// Returns code to initialize the client for testing.
pub fn test_app_init(app_factory: &str) -> String {
    let _ = app_factory;
    "    let client = Client::new();\n    let base_url = \"http://localhost:8080\";\n".to_string()
}

/// Returns the body setup code.
pub fn test_body_setup_code(_body: &RequestBodyDefinition) -> String {
    "    let req_body = json!({});\n".to_string()
}

/// Returns the request builder string.
pub fn test_request_builder(method: &str, uri: &str, body_setup: &str) -> String {
    let mut code = String::new();
    if !body_setup.is_empty() {
        code.push_str(body_setup);
    }
    let lower_method = method.to_lowercase();
    code.push_str(&format!(
        "    let req = client.{}(format!(\"{{}}{{}}\", base_url, \"{}\"))",
        lower_method, uri
    ));
    if !body_setup.is_empty() {
        code.push_str(".json(&req_body)");
    }
    code.push_str(";\n");
    code
}

/// Returns the API call code.
pub fn test_api_call() -> String {
    "    let resp = req.send().await.expect(\"Failed to send request\");\n".to_string()
}

/// Returns assertion code.
pub fn test_assertion() -> String {
    "    assert!(resp.status().is_success());\n".to_string()
}

/// Returns the validation helper.
pub fn test_validation_helper() -> String {
    "".to_string()
}
