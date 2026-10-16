#![deny(missing_docs)]

//! # Axum Strategy Module
//!
//! Implementation of [`CodegenStrategy`], [`HttpServerStrategy`], and [`BackendStrategy`]
//! for the Axum Web framework.

pub mod extractors;
pub mod registration;
pub mod scaffolding;
pub mod testing;

use crate::openapi::parse::models::{ContentMediaType, SecurityRequirementGroup};
use crate::openapi::parse::ParsedRoute;
use crate::strategies::{BackendStrategy, CodegenStrategy, HttpServerStrategy, StrategyKind};

/// Strategy for generating Axum compatible code.
pub struct AxumStrategy;

impl CodegenStrategy for AxumStrategy {
    fn kind(&self) -> StrategyKind {
        StrategyKind::HttpServer
    }

    fn module_imports(&self) -> String {
        scaffolding::handler_imports()
    }

    fn emit_handler(&self, route: &ParsedRoute, args: &[String]) -> String {
        scaffolding::handler_signature(route, args)
    }

    fn extra_cargo_dependencies(&self) -> String {
        r#"axum = "0.7"
axum-extra = { version = "0.9", features = ["typed-header", "cookie"] }
tower = { version = "0.4", features = ["util"] }
"#
        .to_string()
    }
}

impl HttpServerStrategy for AxumStrategy {
    fn server_path_extractor(&self, inner_types: &[String]) -> String {
        extractors::path_extractor(inner_types)
    }

    fn server_query_extractor(&self) -> String {
        extractors::query_extractor()
    }

    fn server_typed_query_extractor(&self, inner_type: &str) -> String {
        extractors::typed_query_extractor(inner_type)
    }

    fn server_query_string_extractor(
        &self,
        inner_type: &str,
        content_media_type: Option<&ContentMediaType>,
    ) -> String {
        extractors::query_string_extractor(inner_type, content_media_type)
    }

    fn server_header_extractor(&self, inner_type: &str) -> String {
        extractors::header_extractor(inner_type)
    }

    fn server_cookie_extractor(&self) -> String {
        extractors::cookie_extractor()
    }

    fn server_body_extractor(&self, body_type: &str) -> String {
        extractors::body_extractor(body_type)
    }

    fn server_form_extractor(&self, body_type: &str) -> String {
        extractors::form_extractor(body_type)
    }

    fn server_multipart_extractor(&self, body_type: &str) -> String {
        extractors::multipart_extractor(body_type)
    }

    fn server_text_extractor(&self, body_type: &str) -> String {
        extractors::text_extractor(body_type)
    }

    fn server_bytes_extractor(&self, body_type: &str) -> String {
        extractors::bytes_extractor(body_type)
    }

    fn server_security_extractor(&self, requirements: &[SecurityRequirementGroup]) -> String {
        extractors::security_extractor(requirements)
    }

    fn server_route_registration(&self, route: &ParsedRoute, handler_full_path: &str) -> String {
        registration::route_registration_statement(route, handler_full_path)
    }
}

impl BackendStrategy for AxumStrategy {
    fn path_extractor(&self, inner_types: &[String]) -> String {
        self.server_path_extractor(inner_types)
    }

    fn query_extractor(&self) -> String {
        self.server_query_extractor()
    }

    fn typed_query_extractor(&self, inner_type: &str) -> String {
        self.server_typed_query_extractor(inner_type)
    }

    fn query_string_extractor(
        &self,
        inner_type: &str,
        content_media_type: Option<&ContentMediaType>,
    ) -> String {
        self.server_query_string_extractor(inner_type, content_media_type)
    }

    fn header_extractor(&self, inner_type: &str) -> String {
        self.server_header_extractor(inner_type)
    }

    fn cookie_extractor(&self) -> String {
        self.server_cookie_extractor()
    }

    fn body_extractor(&self, body_type: &str) -> String {
        self.server_body_extractor(body_type)
    }

    fn form_extractor(&self, body_type: &str) -> String {
        self.server_form_extractor(body_type)
    }

    fn multipart_extractor(&self, body_type: &str) -> String {
        self.server_multipart_extractor(body_type)
    }

    fn text_extractor(&self, body_type: &str) -> String {
        self.server_text_extractor(body_type)
    }

    fn bytes_extractor(&self, body_type: &str) -> String {
        self.server_bytes_extractor(body_type)
    }

    fn security_extractor(&self, requirements: &[SecurityRequirementGroup]) -> String {
        self.server_security_extractor(requirements)
    }

    fn route_registration_statement(&self, route: &ParsedRoute, handler_full_path: &str) -> String {
        self.server_route_registration(route, handler_full_path)
    }

    fn test_imports(&self) -> String {
        testing::test_imports()
    }

    fn test_fn_signature(&self, fn_name: &str) -> String {
        testing::test_fn_signature(fn_name)
    }

    fn test_app_init(&self, app_factory: &str) -> String {
        testing::test_app_init(app_factory)
    }

    fn test_body_setup_code(&self, body: &crate::openapi::parse::RequestBodyDefinition) -> String {
        testing::test_body_setup_code(body)
    }

    fn test_request_builder(&self, method: &str, uri: &str, body_setup: &str) -> String {
        testing::test_request_builder(method, uri, body_setup)
    }

    fn test_api_call(&self) -> String {
        testing::test_api_call()
    }

    fn test_assertion(&self) -> String {
        testing::test_assertion()
    }

    fn test_validation_helper(&self) -> String {
        testing::test_validation_helper()
    }

    fn handler_unit_test(&self, route: &ParsedRoute) -> String {
        testing::handler_unit_test(route)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::RouteKind;
    use std::collections::BTreeMap;

    #[test]
    fn test_axum_handler_imports() {
        let s = AxumStrategy;
        assert_eq!(s.kind(), StrategyKind::HttpServer);
        assert!(s.extra_cargo_dependencies().contains("axum"));
        let imports = s.handler_imports();
        assert!(imports.contains("use axum"));
    }

    #[test]
    fn test_axum_handler_signature() {
        let s = AxumStrategy;

        let route = ParsedRoute {
            path: "/path".into(),
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
            method: "POST".into(),
            handler_name: "get_user".into(),
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
            kind: RouteKind::Path,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            tags: vec![],
            extensions: BTreeMap::new(),
        };

        let code = s.handler_signature(&route, &[]);
        assert!(code.contains("StatusCode::OK"));
    }

    #[test]
    fn test_axum_extractors() {
        let s = AxumStrategy;
        assert_eq!(s.path_extractor(&["i32".to_string()]), "Path<i32>");
        assert_eq!(s.query_extractor(), "Query<serde_json::Value>");
        assert_eq!(s.typed_query_extractor("MyQuery"), "Query<MyQuery>");
        assert_eq!(s.query_string_extractor("MyQuery", None), "Query<MyQuery>");
        assert_eq!(
            s.header_extractor("MyHeader"),
            "axum_extra::TypedHeader<MyHeader>"
        );
        assert_eq!(
            s.cookie_extractor(),
            "axum_extra::extract::cookie::CookieJar"
        );
        assert_eq!(s.body_extractor("MyBody"), "Json<MyBody>");
        assert_eq!(s.form_extractor("MyForm"), "Form<MyForm>");
        assert_eq!(s.multipart_extractor("MyBody"), "Multipart");
        assert_eq!(s.text_extractor("MyBody"), "String");
        assert_eq!(s.bytes_extractor("MyBody"), "axum::body::Bytes");
        assert_eq!(s.security_extractor(&[]), "");
    }

    #[test]
    fn test_axum_route_registration() {
        let s = AxumStrategy;
        let route = ParsedRoute {
            path: "/path".into(),
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
            method: "POST".into(),
            handler_name: "post_handler".into(),
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
            kind: RouteKind::Path,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            tags: vec![],
            extensions: BTreeMap::new(),
        };
        let code = s.route_registration_statement(&route, "mod::ph");
        assert_eq!(code, r#".route("/path", axum::routing::post(mod::ph))"#);
    }

    #[test]
    fn test_axum_test_generation() {
        let s = AxumStrategy;
        assert!(s.test_imports().contains("use axum"));
        assert!(s.test_fn_signature("route").contains("test_route"));
        assert!(s.test_app_init("factory").contains("factory()"));

        let body = crate::openapi::parse::RequestBodyDefinition {
            ty: "MyBody".to_string(),
            description: None,
            media_type: "text/plain".to_string(),
            format: crate::openapi::parse::BodyFormat::Text,
            required: true,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
            example: None,
        };
        assert!(s.test_body_setup_code(&body).contains("Body::from"));
        assert!(s
            .test_request_builder("get", "/", "")
            .contains("Request::builder"));
        assert!(s.test_api_call().contains("oneshot"));
        assert!(s.test_assertion().contains("StatusCode::OK"));
        assert!(s.test_validation_helper().contains("validate_response"));

        let route = ParsedRoute {
            path: "/path".into(),
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
            handler_name: "get_handler".into(),
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
            kind: RouteKind::Path,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            tags: vec![],
            extensions: BTreeMap::new(),
        };
        assert!(s
            .handler_unit_test(&route)
            .contains("test_get_handler_unit"));
    }
}
