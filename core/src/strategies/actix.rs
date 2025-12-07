#![deny(missing_docs)]

//! # Actix Strategy
//!
//! Implementation of `BackendStrategy` for the Actix Web framework.
//! Generates handlers, extractors, and test code tailored to the Actix ecosystem.

use crate::oas::models::SecurityRequirement;
use crate::oas::ParsedRoute;
use crate::strategies::BackendStrategy;

/// Strategy for generating Actix Web compatible code.
pub struct ActixStrategy;

impl BackendStrategy for ActixStrategy {
    // --- Scaffolding & Routing ---

    fn handler_imports(&self) -> String {
        let mut imports = String::new();
        imports.push_str("use actix_web::{web, HttpResponse, Responder};\n");
        // We include actix_multipart generic import just in case, though usually requires separate crate dependency
        imports.push_str("use actix_multipart::Multipart;\n");
        imports.push_str("use serde_json::Value;\n");
        imports.push_str("use uuid::Uuid;\n");
        imports.push_str("use chrono::{DateTime, Utc, NaiveDate, NaiveDateTime};\n");
        // We assume the target application has a `security` module for auth types if used
        imports
    }

    fn handler_signature(
        &self,
        func_name: &str,
        args: &[String],
        response_type: Option<&str>,
    ) -> String {
        let args_str = args.join(", ");
        let return_type = if let Some(rt) = response_type {
            // Strictly typed return: actix_web::Result<web::Json<T>>
            format!("actix_web::Result<web::Json<{}>>", rt)
        } else {
            "impl Responder".to_string()
        };

        format!(
            "pub async fn {}({}) -> {} {{\n    todo!()\n}}\n",
            func_name, args_str, return_type
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

    fn header_extractor(&self, _inner_type: &str) -> String {
        "web::Header<String>".to_string()
    }

    fn cookie_extractor(&self) -> String {
        "web::Cookie".to_string()
    }

    fn body_extractor(&self, body_type: &str) -> String {
        format!("web::Json<{}>", body_type)
    }

    fn form_extractor(&self, body_type: &str) -> String {
        format!("web::Form<{}>", body_type)
    }

    fn multipart_extractor(&self) -> String {
        "Multipart".to_string()
    }

    fn security_extractor(&self, requirements: &[SecurityRequirement]) -> String {
        if requirements.is_empty() {
            return "".to_string();
        }

        // OAS allows multiple security requirement objects (OR logic).
        // e.g. [{ApiKey: []}, {OAuth: [read]}]
        // We generate an extractor for the FIRST requirement set to define the primary auth path.
        let req = &requirements[0];
        let scheme = to_pascal_case(&req.scheme_name);

        if req.scopes.is_empty() {
            // Simple Auth: just the scheme
            // e.g. _auth: web::ReqData<security::ApiKey>
            format!("_auth: web::ReqData<security::{}>", scheme)
        } else {
            // Scoped Auth: Scheme + Scopes
            // e.g. _auth: web::ReqData<security::Authenticated<security::OAuth, (security::scopes::Read, security::scopes::Write)>>

            let normalized_scopes: Vec<String> = req
                .scopes
                .iter()
                .map(|s| format!("security::scopes::{}", to_pascal_case(s)))
                .collect();

            let scopes_tuple = if normalized_scopes.len() == 1 {
                normalized_scopes[0].clone()
            } else {
                format!("({})", normalized_scopes.join(", "))
            };

            format!(
                "_auth: web::ReqData<security::Authenticated<security::{}, {}>>",
                scheme, scopes_tuple
            )
        }
    }

    fn route_registration_statement(&self, route: &ParsedRoute, handler_full_path: &str) -> String {
        let method = route.method.to_uppercase();
        let actix_method = match method.as_str() {
            "GET" => "get()".to_string(),
            "POST" => "post()".to_string(),
            "PUT" => "put()".to_string(),
            "DELETE" => "delete()".to_string(),
            "PATCH" => "patch()".to_string(),
            "HEAD" => "head()".to_string(),
            "TRACE" => "trace()".to_string(),
            "OPTIONS" => "options()".to_string(),
            // "QUERY" is OAS 3.2.0 specific and Actix might not have a helper.
            "QUERY" => {
                "method(actix_web::http::Method::from_bytes(b\"QUERY\").unwrap())".to_string()
            }
            // Fallback for unknown standard verbs or extensions
            str_method => format!(
                "method(actix_web::http::Method::from_bytes(b\"{}\").unwrap())",
                str_method
            ),
        };

        format!(
            "\n    cfg.service(web::resource(\"{}\").route(web::{}.to({})));",
            route.path, actix_method, handler_full_path
        )
    }

    // --- Test Generation ---

    fn test_imports(&self) -> String {
        let mut code = String::new();
        code.push_str("#![allow(unused_imports, unused_variables, dead_code)]\n\n");
        code.push_str("use actix_web::{test, App, web};\n");
        code.push_str("use serde_json::Value;\n");
        code.push_str("use std::fs;\n");
        code.push_str("use jsonschema::JSONSchema;\n\n");
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
        // Actix TestRequest helpers exist for standard verbs: get(), post(), put(), delete(), patch().
        // For others, we use method().
        let builder_call = match method_lower.as_str() {
            "get" | "post" | "put" | "delete" | "patch" => format!("{}()", method_lower),
            // Explicitly handle query to ensure robust test generation for 3.2.0 spec
            "query" => {
                "method(actix_web::http::Method::from_bytes(b\"QUERY\").unwrap())".to_string()
            }
            // TestRequest::head() depends on crate version, explicit method is safest
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

    fn test_api_call(&self) -> String {
        "    let resp = test::call_service(&app, req).await;".to_string()
    }

    fn test_assertion(&self) -> String {
        "    assert_ne!(resp.status(), actix_web::http::StatusCode::NOT_FOUND, \"Route should exist\");".to_string()
    }

    fn test_validation_helper(&self) -> String {
        r#"
/// Helper to validate response body against OpenAPI schema.
/// Supports both OpenAPI 3.x (content -> media -> schema) and Swagger 2.0 (schema only).
async fn validate_response(resp: actix_web::dev::ServiceResponse, method: &str, path_template: &str) {
    use actix_web::body::MessageBody;

    // 1. Extract Body
    let body_bytes = resp.into_body().try_into_bytes().expect("Failed to read response body");
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null);

    // 2. Read OpenAPI
    let yaml_content = fs::read_to_string(OPENAPI_PATH).expect("Failed to read openapi.yaml");
    let openapi: serde_json::Value = serde_yaml::from_str(&yaml_content).expect("Failed to parse OpenAPI");

    // 3. Navigate to Response Schema
    let status_str = "200"; // Assuming success for contract test defaults, normally check resp.status()
    let method_key = method.to_lowercase();

    let operation = openapi.get("paths")
        .and_then(|p| p.get(path_template))
        .and_then(|path_item| path_item.get(&method_key));

    if let Some(op) = operation {
        let responses = op.get("responses");
        let response = responses.and_then(|r| r.get(status_str).or_else(|| r.get("default")));

        if let Some(resp_def) = response {
            // Lookup Strategy:
            // 1. OpenAPI 3.x: content -> application/json -> schema
            // 2. Swagger 2.0: schema (direct)
            let schema_oas3 = resp_def.get("content")
                .and_then(|c| c.get("application/json"))
                .and_then(|m| m.get("schema"));

            let schema_swagger2 = resp_def.get("schema");

            if let Some(schema) = schema_oas3.or(schema_swagger2) {
                // 4. Compile and Validate
                // We compile the specific schema fragment.
                // Note: If schema uses local $refs (#/components/...), JSONSchema compile might succeed
                // if it doesn't need to resolve them immediately, or fail if it does.
                match JSONSchema::options().compile(schema) {
                    Ok(validator) => {
                        if let Err(errors) = validator.validate(&body_json) {
                            let err_msgs: Vec<String> = errors.map(|e| e.to_string()).collect();
                            panic!(
                                "Response schema validation failed for {} {}\nErrors:\n{}",
                                method, path_template, err_msgs.join("\n")
                            );
                        }
                    }
                    Err(e) => {
                        // If compilation fails (e.g. missing refs), strictly failing the test
                        // ensures the user is aware their spec might need to be flattened or fixed for testing.
                        panic!("Failed to compile JSON Schema from spec: {}", e);
                    }
                }
            }
        }
    }
}
"#
            .to_string()
    }
}

/// Helper to covert to PascalCase for types
fn to_pascal_case(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut c = part.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::{RouteKind, SecurityRequirement};
    use crate::oas::ParsedRoute;

    #[test]
    fn test_actix_handler_imports() {
        let s = ActixStrategy;
        let imports = s.handler_imports();
        assert!(imports.contains("use actix_web"));
        assert!(imports.contains("use uuid::Uuid"));
        assert!(imports.contains("use actix_multipart::Multipart"));
    }

    #[test]
    fn test_actix_handler_signature_generic() {
        let s = ActixStrategy;
        let sig = s.handler_signature(
            "my_handler",
            &["id: usize".into(), "q: String".into()],
            None,
        );
        assert_eq!(
            sig,
            "pub async fn my_handler(id: usize, q: String) -> impl Responder {\n    todo!()\n}\n"
        );
    }

    #[test]
    fn test_actix_handler_signature_typed() {
        let s = ActixStrategy;
        let sig = s.handler_signature("get_user", &["id: usize".into()], Some("User"));
        assert_eq!(
            sig,
            "pub async fn get_user(id: usize) -> actix_web::Result<web::Json<User>> {\n    todo!()\n}\n"
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
    fn test_actix_security_extractor_simple() {
        let s = ActixStrategy;
        let reqs = vec![SecurityRequirement {
            scheme_name: "ApiKey".into(),
            scopes: vec![],
        }];
        // Expect: _auth: web::ReqData<security::ApiKey>
        assert_eq!(
            s.security_extractor(&reqs),
            "_auth: web::ReqData<security::ApiKey>"
        );
    }

    #[test]
    fn test_actix_security_extractor_scopes() {
        let s = ActixStrategy;
        let reqs = vec![SecurityRequirement {
            scheme_name: "oAuth2".into(),
            scopes: vec!["read:user".into(), "write:admin".into()],
        }];

        let output = s.security_extractor(&reqs);
        assert!(output.contains("_auth: web::ReqData<security::Authenticated"));
        assert!(output.contains("security::OAuth2"));
        assert!(output.contains("security::scopes::ReadUser"));
        assert!(output.contains("security::scopes::WriteAdmin"));
    }

    #[test]
    fn test_actix_query_extractor() {
        let s = ActixStrategy;
        assert_eq!(s.query_extractor(), "web::Query<Value>");
    }

    #[test]
    fn test_actix_header_extractor() {
        let s = ActixStrategy;
        assert_eq!(s.header_extractor("String"), "web::Header<String>");
    }

    #[test]
    fn test_actix_cookie_extractor() {
        let s = ActixStrategy;
        assert_eq!(s.cookie_extractor(), "web::Cookie");
    }

    #[test]
    fn test_actix_body_extractor() {
        let s = ActixStrategy;
        assert_eq!(s.body_extractor("MyDto"), "web::Json<MyDto>");
    }

    #[test]
    fn test_actix_form_extractor() {
        let s = ActixStrategy;
        assert_eq!(s.form_extractor("SearchForm"), "web::Form<SearchForm>");
    }

    #[test]
    fn test_actix_multipart_extractor() {
        let s = ActixStrategy;
        assert_eq!(s.multipart_extractor(), "Multipart");
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
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
            callbacks: vec![],
        };
        let code = s.route_registration_statement(&route, "mod::handler");
        assert_eq!(
            code,
            "\n    cfg.service(web::resource(\"/path\").route(web::post().to(mod::handler)));"
        );
    }

    #[test]
    fn test_actix_route_registration_custom_verb() {
        let s = ActixStrategy;
        let route = ParsedRoute {
            path: "/path".into(),
            method: "QUERY".into(),
            handler_name: "query_handler".into(),
            params: vec![],
            request_body: None,
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
            callbacks: vec![],
        };
        let code = s.route_registration_statement(&route, "mod::qh");
        assert!(code.contains(
            ".route(web::method(actix_web::http::Method::from_bytes(b\"QUERY\").unwrap()).to(mod::qh)));"
        ));
    }

    #[test]
    fn test_actix_test_generation_components() {
        let s = ActixStrategy;
        assert!(s.test_imports().contains("use actix_web"));
        assert!(s.test_imports().contains("use jsonschema::JSONSchema;"));
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
        assert!(s
            .test_validation_helper()
            .contains("JSONSchema::options().compile(schema)"));
    }

    #[test]
    fn test_actix_test_generation_custom_method() {
        let s = ActixStrategy;
        let req = s.test_request_builder("HEAD", "/uri", "");
        assert!(req.contains(
            "test::TestRequest::method(actix_web::http::Method::from_bytes(b\"HEAD\").unwrap())"
        ));
        assert!(req.contains(".uri(\"/uri\")"));
    }

    #[test]
    fn test_actix_test_generation_http_query_method() {
        let s = ActixStrategy;
        let req = s.test_request_builder("QUERY", "/search", "");
        assert!(req.contains(
            "test::TestRequest::method(actix_web::http::Method::from_bytes(b\"QUERY\").unwrap())"
        ));
        assert!(req.contains(".uri(\"/search\")"));
    }
}
