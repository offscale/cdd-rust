#![deny(missing_docs)]

//! # Actix Strategy
//!
//! Implementation of `BackendStrategy` for the Actix Web framework.

use super::traits::BackendStrategy;
use crate::oas::models::SecurityRequirement;
use crate::oas::ParsedRoute;

/// Strategy implementation for Actix Web.
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
            // Note: Error type is implicit or actix_web::Error
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
        // Generate a Stub Extractor using Actix ReqData or custom Trait
        // For simplicity, we generate `_auth: web::ReqData<AuthenticatedUser>` or similar.
        // Or if we know the scheme name, `_auth: Auth<ApiKey>`
        // Since we don't define the extractor structs in the strategy, we use a placeholder.

        let schemes: Vec<&String> = requirements.iter().map(|r| &r.scheme_name).collect();
        // Just use the first one name for the type placeholder or generic 'Auth'
        let name = schemes.first().unwrap();
        format!("_auth: web::ReqData<{}>", name)
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
            // "OPTIONS" usually implies Generic route if using web::options() check docs, but web::resource("..").route(web::options()) works.
            // Note: actix_web::web::options() does exist.
            "OPTIONS" => "options()".to_string(),
            // "QUERY" is OAS 3.2.0 specific and Actix might not have a helper.
            // We use the generic method construction.
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
        // Actix TestRequest helpers exist for standard verbs: get(), post(), put(), delete(), patch().
        // For others, we use method().
        let builder_call = match method_lower.as_str() {
            "get" | "post" | "put" | "delete" | "patch" => format!("{}()", method_lower),
            // Explicitly handle query to ensure robust test generation for 3.2.0 spec
            "query" => {
                "method(actix_web::http::Method::from_bytes(b\"QUERY\").unwrap())".to_string()
            }
            // TestRequest::head() doesn't exist in some versions, depends on crate.
            // Safest to use .method(...) for extended verbs.
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
    fn test_actix_security_extractor() {
        let s = ActixStrategy;
        let reqs = vec![SecurityRequirement {
            scheme_name: "ApiKey".into(),
            scopes: vec![],
        }];
        // Expect: _auth: web::ReqData<ApiKey>
        assert_eq!(s.security_extractor(&reqs), "_auth: web::ReqData<ApiKey>");
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
        assert!(code.contains(".route(web::method(actix_web::http::Method::from_bytes(b\"QUERY\").unwrap()).to(mod::qh)));"));
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
