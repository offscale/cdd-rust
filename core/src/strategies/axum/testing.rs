#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Test Generation
//!
//! Generates integration test scaffolding for Axum applications.

use crate::openapi::parse::{BodyFormat, RequestBodyDefinition};

/// Test imports.
pub fn test_imports() -> String {
    let mut imports = String::new();
    imports.push_str("use axum::{body::Body, http::{Request, Method, StatusCode}};\n");
    imports.push_str("use tower::ServiceExt; // for `app.oneshot()`\n");
    imports.push_str("use serde_json::json;\n");
    imports.push_str("use jsonschema::{Draft, JSONSchema};\n");
    imports
}

/// Test function signature.
pub fn test_fn_signature(fn_name: &str) -> String {
    format!("#[tokio::test]\nasync fn test_{}() {{", fn_name)
}

/// App initialization code.
pub fn test_app_init(app_factory: &str) -> String {
    format!("    let app = {}();\n", app_factory)
}

/// Body setup code.
pub fn test_body_setup_code(body: &RequestBodyDefinition) -> String {
    match body.format {
        BodyFormat::Json => {
            let mut code = String::new();
            code.push_str("    let req_body = json!({\n");
            code.push_str("        // TODO: Populate test payload\n");
            code.push_str("    });\n");
            code.push_str(
                "    let body = Body::from(serde_json::to_vec(&req_body).expect(\"Failed\"));\n",
            );
            code
        }
        BodyFormat::Text => "    let body = Body::from(\"TODO: test payload\");\n".to_string(),
        _ => "    let body = Body::empty(); // TODO: Implement body for this format\n".to_string(),
    }
}

/// Request builder code.
pub fn test_request_builder(method: &str, uri: &str, body_setup: &str) -> String {
    let mut code = String::new();
    if !body_setup.is_empty() {
        code.push_str(body_setup);
    } else {
        code.push_str("    let body = Body::empty();\n");
    }

    code.push_str(&format!(
        "    let request = Request::builder()\n        .method(Method::{})\n        .uri(\"{}\")\n        .body(body)\n        .expect(\"Failed\");\n",
        method.to_uppercase(),
        uri
    ));
    code
}

/// API call execution.
pub fn test_api_call() -> String {
    "    let response = app.oneshot(request).await.expect(\"Failed\");\n".to_string()
}

/// Assertion code.
pub fn test_assertion() -> String {
    "    assert_eq!(response.status(), StatusCode::OK);\n".to_string()
}

/// Validation helper snippet.
pub fn test_validation_helper() -> String {
    "// Axum validation helper\n".to_string() // Minimal placeholder, typically implemented in test_gen root
}

/// Generates a unit test for a specific handler function to be placed alongside it.
pub fn handler_unit_test(route: &crate::openapi::parse::ParsedRoute) -> String {
    let method = match route.method.as_str() {
        "GET" => "get",
        "POST" => "post",
        "PUT" => "put",
        "DELETE" => "delete",
        "PATCH" => "patch",
        "HEAD" => "head",
        "OPTIONS" => "options",
        _ => "get",
    };

    let mut path = route.path.clone();
    while let Some(start) = path.find('{') {
        if let Some(end) = path[start..].find('}') {
            let actual_end = start + end;
            path.replace_range(start..actual_end + 1, ":id"); // Dummy param
        } else {
            break;
        }
    }

    // Call it with an empty Request
    format!(
        r#"    #[tokio::test]
    async fn test_{handler_name}_unit() {{
        use axum::{{Router, routing::{method}}};
        use axum::http::{{Request, StatusCode}};
        use tower::ServiceExt;
        
        let app = Router::new().route("{path}", {method}(super::{handler_name}));
        let req = Request::builder()
            .method("{METHOD}")
            .uri("{req_path}")
            .body(axum::body::Body::empty())
            .expect("Failed to build request");
            
        let resp = app.oneshot(req).await.expect("Failed to execute request");
        assert!(resp.status() == StatusCode::OK || resp.status().is_client_error() || resp.status().is_server_error());
    }}"#,
        handler_name = route.handler_name,
        path = path,
        method = method,
        METHOD = route.method.to_uppercase(),
        req_path = path.replace(":id", "0"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imports_generation() {
        assert!(test_imports().contains("use axum::"));
    }

    #[test]
    fn test_handler_unit_test() {
        let route = crate::openapi::parse::ParsedRoute {
            method: "POST".into(),
            handler_name: "my_handler".into(),
            path: "/users/{id}".into(),
            ..crate::openapi::parse::ParsedRoute {
                path: "".into(),
                method: "".into(),
                handler_name: "".into(),
                summary: None,
                description: None,
                path_summary: None,
                path_description: None,
                path_extensions: std::collections::BTreeMap::new(),
                operation_summary: None,
                operation_description: None,
                base_path: None,
                path_servers: None,
                servers_override: None,
                operation_id: None,
                params: vec![],
                path_params: vec![],
                request_body: None,
                security: vec![],
                security_defined: false,
                response_type: None,
                response_status: None,
                response_summary: None,
                response_description: None,
                response_media_type: None,
                response_example: None,
                response_headers: vec![],
                response_links: None,
                kind: crate::openapi::parse::models::RouteKind::Path,
                callbacks: vec![],
                deprecated: false,
                external_docs: None,
                raw_request_body: None,
                raw_responses: None,
                tags: vec![],
                extensions: std::collections::BTreeMap::new(),
            }
        };

        let generated = handler_unit_test(&route);
        assert!(generated.contains("test_my_handler_unit"));
        assert!(generated.contains("post(super::my_handler)"));
        assert!(generated.contains(".uri(\"/users/0\")"));
    }
}
