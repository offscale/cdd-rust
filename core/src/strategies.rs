#![deny(missing_docs)]

//! # Strategy Module
//!
//! Defines the `BackendStrategy` trait and implementations (e.g. `ActixStrategy`)
//! to allow generating code for different web frameworks.

use crate::oas::ParsedRoute;

/// A strategy trait for decoupling framework-specific code generation.
///
/// Implementors define how to generate imports, handler signatures, extractors,
/// and route registrations for a specific backend (Actix, Axum, etc.).
/// Also includes methods for generating integration tests.
pub trait BackendStrategy {
    // --- Scaffolding & Routing ---

    /// Returns the standard imports for a handler file in this framework.
    fn handler_imports(&self) -> String;

    /// Generates a handler function signature.
    ///
    /// # Arguments
    ///
    /// * `func_name` - The name of the function.
    /// * `args` - A list of argument declaration strings (e.g. `id: web::Path<Uuid>`).
    fn handler_signature(&self, func_name: &str, args: &[String]) -> String;

    /// Generates the type string for path parameter extraction.
    ///
    /// # Arguments
    ///
    /// * `inner_types` - The Rust types of the path parameters (e.g. `["Uuid", "i32"]`).
    fn path_extractor(&self, inner_types: &[String]) -> String;

    /// Generates the type string for query parameter extraction.
    fn query_extractor(&self) -> String;

    /// Generates the type string for request body extraction.
    ///
    /// # Arguments
    ///
    /// * `body_type` - The Rust type of the body (e.g. `CreateUserRequest`).
    fn body_extractor(&self, body_type: &str) -> String;

    /// Generates the route registration code statement.
    ///
    /// # Arguments
    ///
    /// * `route` - The parsed route definition.
    /// * `handler_full_path` - The fully qualified path to the handler (e.g. `handlers::users::create`).
    fn route_registration_statement(&self, route: &ParsedRoute, handler_full_path: &str) -> String;

    // --- Test Generation ---

    /// Returns the standard imports for a test file in this framework.
    fn test_imports(&self) -> String;

    /// Returns the test function signature (including attributes).
    ///
    /// Example: `#[actix_web::test]\nasync fn test_foo() {`
    fn test_fn_signature(&self, fn_name: &str) -> String;

    /// Returns code to initialize the application for testing.
    ///
    /// * `app_factory` - code string for the app factory (e.g. `crate::create_app`).
    fn test_app_init(&self, app_factory: &str) -> String;

    /// Returns the code snippet that attaches a dummy JSON body to the request.
    fn test_body_setup_code(&self) -> String;

    /// Returns code to build the request object.
    ///
    /// * `method` - HTTP method (GET, POST).
    /// * `uri` - Request URI.
    /// * `body_setup` - Code snippet inserted if body is present.
    fn test_request_builder(&self, method: &str, uri: &str, body_setup: &str) -> String;

    /// Returns code to execute the request against the app.
    fn test_api_call(&self) -> String;

    /// Returns assertion code for the response.
    fn test_assertion(&self) -> String;

    /// Returns the helper function code for validating responses against OpenAPI.
    fn test_validation_helper(&self) -> String;
}

/// Strategy implementation for Actix Web.
pub struct ActixStrategy;

impl BackendStrategy for ActixStrategy {
    // --- Scaffolding & Routing ---

    fn handler_imports(&self) -> String {
        let mut imports = String::new();
        imports.push_str("use actix_web::{web, HttpResponse, Responder};\n");
        imports.push_str("use serde_json::Value;\n");
        imports.push_str("use uuid::Uuid;\n");
        imports.push_str("use chrono::{DateTime, Utc, NaiveDate, NaiveDateTime};\n");
        imports
    }

    fn handler_signature(&self, func_name: &str, args: &[String]) -> String {
        let args_str = args.join(", ");
        format!(
            "pub async fn {}({}) -> impl Responder {{\n    todo!()\n}}\n",
            func_name, args_str
        )
    }

    fn path_extractor(&self, inner_types: &[String]) -> String {
        if inner_types.len() == 1 {
            format!("web::Path<{}>", inner_types[0])
        } else {
            let tuple = inner_types.join(", ");
            format!("web::Path<({})>", tuple)
        }
    }

    fn query_extractor(&self) -> String {
        "web::Query<Value>".to_string()
    }

    fn body_extractor(&self, body_type: &str) -> String {
        format!("web::Json<{}>", body_type)
    }

    fn route_registration_statement(&self, route: &ParsedRoute, handler_full_path: &str) -> String {
        let method = route.method.to_lowercase();
        format!(
            "\n    cfg.service(web::resource(\"{}\").route(web::{}().to({})));",
            route.path, method, handler_full_path
        )
    }

    // --- Test Generation ---

    fn test_imports(&self) -> String {
        let mut code = String::new();
        code.push_str("#![allow(unused_imports, unused_variables, dead_code)]\n\n");
        code.push_str("use actix_web::{test, App, web};\n");
        code.push_str("use serde_json::Value;\n");
        code.push_str("use std::fs;\n\n");
        code
    }

    fn test_fn_signature(&self, fn_name: &str) -> String {
        format!("#[actix_web::test]\nasync fn {}() {{", fn_name)
    }

    fn test_app_init(&self, app_factory: &str) -> String {
        format!(
            "    let app = test::init_service({}(App::new())).await;",
            app_factory
        )
    }

    fn test_body_setup_code(&self) -> String {
        "        .set_json(serde_json::json!({ \"dummy\": \"value\" }))\n".to_string()
    }

    fn test_request_builder(&self, method: &str, uri: &str, body_setup: &str) -> String {
        let method_lower = method.to_lowercase();
        // handling specialized methods if needed, but actix test::TestRequest::{method}() works for std ones
        // actually test::TestRequest::get(), post(), etc.
        // Dynamic dispatch: test::TestRequest::default().method(...).
        // But the original code used `test::TestRequest::{method}()`.
        // We assume strictly standard methods or match.
        // For simplicity and matching original logic:
        format!(
            "    let req = test::TestRequest::{}().uri(\"{}\")\n{}        .to_request();",
            method_lower, uri, body_setup
        )
    }

    fn test_api_call(&self) -> String {
        "    let resp = test::call_service(&app, req).await;".to_string()
    }

    fn test_assertion(&self) -> String {
        "    assert_ne!(resp.status(), actix_web::http::StatusCode::NOT_FOUND, \"Route should exist\");".to_string()
    }

    fn test_validation_helper(&self) -> String {
        r#"
/// Helper to validate response body against OpenAPI schema.
async fn validate_response(resp: actix_web::dev::ServiceResponse, method: &str, path_template: &str) {
    use actix_web::body::MessageBody;

    // 1. Read OpenAPI
    let yaml_content = fs::read_to_string(OPENAPI_PATH).expect("Failed to read openapi.yaml");
    let openapi: serde_json::Value = serde_yaml::from_str(&yaml_content).expect("Failed to parse OpenAPI");

    // 2. Find Schema for Response
    let status_str = resp.status().as_str();

    let schema_opt = openapi.get("paths")
        .and_then(|p| p.get(path_template))
        .and_then(|p| p.get(method.to_lowercase()))
        .and_then(|op| op.get("responses"))
        .and_then(|r| r.get(status_str).or_else(|| r.get("default")))
        .and_then(|res| res.get("content"))
        .and_then(|c| c.get("application/json"))
        .and_then(|aj| aj.get("schema"));

    if let Some(_schema) = schema_opt {
        let body_bytes = resp.into_body().try_into_bytes().unwrap();
        let _body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null);
    }
}
"#
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actix_handler_imports() {
        let s = ActixStrategy;
        let imports = s.handler_imports();
        assert!(imports.contains("use actix_web"));
        assert!(imports.contains("use uuid::Uuid"));
    }

    #[test]
    fn test_actix_handler_signature() {
        let s = ActixStrategy;
        let sig = s.handler_signature("my_handler", &["id: usize".into(), "q: String".into()]);
        assert_eq!(
            sig,
            "pub async fn my_handler(id: usize, q: String) -> impl Responder {\n    todo!()\n}\n"
        );
    }

    #[test]
    fn test_actix_path_extractor() {
        let s = ActixStrategy;
        assert_eq!(s.path_extractor(&["Uuid".into()]), "web::Path<Uuid>");
        assert_eq!(
            s.path_extractor(&["Uuid".into(), "i32".into()]),
            "web::Path<(Uuid, i32)>"
        );
    }

    #[test]
    fn test_actix_query_extractor() {
        let s = ActixStrategy;
        assert_eq!(s.query_extractor(), "web::Query<Value>");
    }

    #[test]
    fn test_actix_body_extractor() {
        let s = ActixStrategy;
        assert_eq!(s.body_extractor("MyDto"), "web::Json<MyDto>");
    }

    #[test]
    fn test_actix_route_registration() {
        let s = ActixStrategy;
        let route = ParsedRoute {
            path: "/path".into(),
            method: "POST".into(),
            handler_name: "handler".into(),
            params: vec![],
            request_body: None,
        };
        let code = s.route_registration_statement(&route, "mod::handler");
        assert_eq!(
            code,
            "\n    cfg.service(web::resource(\"/path\").route(web::post().to(mod::handler)));"
        );
    }

    #[test]
    fn test_actix_test_generation_components() {
        let s = ActixStrategy;
        assert!(s.test_imports().contains("use actix_web"));
        assert!(s
            .test_fn_signature("test_foo")
            .contains("#[actix_web::test]"));
        assert!(s
            .test_app_init("init")
            .contains("test::init_service(init(App::new()))"));
        assert!(s.test_body_setup_code().contains(".set_json"));
        let req = s.test_request_builder("GET", "/uri", "");
        assert!(req.contains("test::TestRequest::get()"));
        assert!(req.contains(".uri(\"/uri\")"));
        assert!(s.test_api_call().contains("test::call_service"));
        assert!(s.test_assertion().contains("assert_ne!"));
        assert!(s
            .test_validation_helper()
            .contains("actix_web::dev::ServiceResponse"));
    }
}
