#![deny(missing_docs)]

//! # Test Generation
//!
//! Logic for generating integration test code helpers and methods.

/// Returns imports for test files.
pub fn test_imports() -> String {
    let mut code = String::new();
    code.push_str("#![allow(unused_imports, unused_variables, dead_code)]\n\n");
    code.push_str("use actix_web::{test, App, web};\n");
    code.push_str("use serde_json::Value;\n");
    code.push_str("use std::fs;\n");
    code.push_str("use jsonschema::JSONSchema;\n\n");
    code
}

/// Generates test function signature.
pub fn test_fn_signature(fn_name: &str) -> String {
    format!("#[actix_web::test]\nasync fn {}() {{", fn_name)
}

/// Generates app init code.
pub fn test_app_init(app_factory: &str) -> String {
    format!(
        "    let app = test::init_service({}(App::new())).await;",
        app_factory
    )
}

/// Generates body setup stub.
pub fn test_body_setup_code() -> String {
    "        .set_json(serde_json::json!({ \"dummy\": \"value\" }))\n".to_string()
}

/// Generates request builder chain.
pub fn test_request_builder(method: &str, uri: &str, body_setup: &str) -> String {
    let method_lower = method.to_lowercase();
    let builder_call = match method_lower.as_str() {
        "get" | "post" | "put" | "delete" | "patch" => format!("{}()", method_lower),
        "query" => "method(actix_web::http::Method::from_bytes(b\"QUERY\").unwrap())".to_string(),
        _ => format!(
            "method(actix_web::http::Method::from_bytes(b\"{}\").unwrap())",
            method.to_uppercase()
        ),
    };

    format!(
        "    let req = test::TestRequest::{}.uri(\"{}\")\n{}        .to_request();",
        builder_call, uri, body_setup
    )
}

/// Generates call service line.
pub fn test_api_call() -> String {
    "    let resp = test::call_service(&app, req).await;".to_string()
}

/// Generates assertions.
pub fn test_assertion() -> String {
    "    assert_ne!(resp.status(), actix_web::http::StatusCode::NOT_FOUND, \"Route should exist\");"
        .to_string()
}

/// Generates validation helper function.
pub fn test_validation_helper() -> String {
    r#"
/// Helper to validate response body against OpenAPI schema.
async fn validate_response(resp: actix_web::dev::ServiceResponse, method: &str, path_template: &str) {
    use actix_web::body::MessageBody;
    let body_bytes = resp.into_body().try_into_bytes().expect("Failed to read response body");
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null);
    let yaml_content = fs::read_to_string(OPENAPI_PATH).expect("Failed to read openapi.yaml");
    let openapi: serde_json::Value = serde_yaml::from_str(&yaml_content).expect("Failed to parse OpenAPI");

    let status_str = "200";
    let method_key = method.to_lowercase();
    let operation = openapi.get("paths")
        .and_then(|p| p.get(path_template))
        .and_then(|path_item| path_item.get(&method_key));

    if let Some(op) = operation {
        let responses = op.get("responses");
        let response = responses.and_then(|r| r.get(status_str).or_else(|| r.get("default")));
        if let Some(resp_def) = response {
            let schema_oas3 = resp_def.get("content")
                .and_then(|c| c.get("application/json"))
                .and_then(|m| m.get("schema"));
            let schema_swagger2 = resp_def.get("schema");

            if let Some(schema) = schema_oas3.or(schema_swagger2) {
                match JSONSchema::options().compile(schema) {
                    Ok(validator) => {
                        if let Err(errors) = validator.validate(&body_json) {
                            let err_msgs: Vec<String> = errors.map(|e| e.to_string()).collect();
                            panic!("Response schema validation failed: {}", err_msgs.join("\n"));
                        }
                    }
                    Err(e) => panic!("Failed to compile JSON Schema: {}", e),
                }
            }
        }
    }
}
"#
        .to_string()
}
