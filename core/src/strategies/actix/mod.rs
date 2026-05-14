#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Actix Strategy Module
//!
//! Implementation of `BackendStrategy` for the Actix Web framework.
//! Relies on submodules for specific generation logic (extractors, routing, testing, etc.).

pub mod extractors;
pub mod links;
pub mod registration;
pub mod scaffolding;
pub mod testing;
pub mod utils;

use crate::openapi::parse::ParsedRoute;
use crate::strategies::BackendStrategy;

/// Strategy for generating Actix Web compatible code.
pub struct ActixStrategy;

impl BackendStrategy for ActixStrategy {
    // --- Scaffolding ---

    fn handler_imports(&self) -> String {
        scaffolding::handler_imports()
    }

    fn handler_signature(&self, route: &ParsedRoute, args: &[String]) -> String {
        scaffolding::handler_signature(route, args)
    }

    fn path_extractor(&self, inner_types: &[String]) -> String {
        extractors::path_extractor(inner_types)
    }

    fn query_extractor(&self) -> String {
        extractors::query_extractor()
    }

    fn typed_query_extractor(&self, inner_type: &str) -> String {
        extractors::typed_query_extractor(inner_type)
    }

    fn query_string_extractor(
        &self,
        inner_type: &str,
        content_media_type: Option<&crate::openapi::parse::models::ContentMediaType>,
    ) -> String {
        extractors::query_string_extractor(inner_type, content_media_type)
    }

    fn header_extractor(&self, inner_type: &str) -> String {
        extractors::header_extractor(inner_type)
    }

    fn cookie_extractor(&self) -> String {
        extractors::cookie_extractor()
    }

    fn body_extractor(&self, body_type: &str) -> String {
        extractors::body_extractor(body_type)
    }

    fn form_extractor(&self, body_type: &str) -> String {
        extractors::form_extractor(body_type)
    }

    fn multipart_extractor(&self, body_type: &str) -> String {
        extractors::multipart_extractor(body_type)
    }

    fn text_extractor(&self, body_type: &str) -> String {
        extractors::text_extractor(body_type)
    }

    fn bytes_extractor(&self, body_type: &str) -> String {
        extractors::bytes_extractor(body_type)
    }

    fn security_extractor(
        &self,
        requirements: &[crate::openapi::parse::models::SecurityRequirementGroup],
    ) -> String {
        extractors::security_extractor(requirements)
    }

    fn route_registration_statement(&self, route: &ParsedRoute, handler_full_path: &str) -> String {
        registration::route_registration_statement(route, handler_full_path)
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
    use crate::openapi::parse::ParsedRoute;
    use std::collections::BTreeMap;

    #[test]
    fn test_actix_handler_imports() {
        let s = ActixStrategy;
        let imports = s.handler_imports();
        assert!(imports.contains("use actix_web"));
        assert!(imports.contains("use uuid::Uuid"));
    }

    #[test]
    fn test_handler_signature_passes_through() {
        let s = ActixStrategy;

        // Use a dummy route just to make it compile
        let route = ParsedRoute {
            path: "/users/{id}".into(),
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
            handler_name: "get_user".into(),
            operation_id: None,
            params: vec![],
            path_params: vec![],
            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: Some("User".into()),
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let code = s.handler_signature(&route, &["id: Uuid".to_string()]);

        assert!(code.contains("pub async fn get_user"));
        assert!(code.contains("id: Uuid"));
    }

    #[test]
    fn test_actix_route_registration_custom_verb() {
        let s = ActixStrategy;
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
            method: "QUERY".into(),
            handler_name: "query_handler".into(),
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
        let code = s.route_registration_statement(&route, "mod::qh");
        assert!(code.contains(
            ".route(web::method(actix_web::http::Method::from_bytes(b\"QUERY\").expect(\"expected value\")).to(mod::qh)));"
        ));
    }

    #[test]
    fn test_actix_test_generation_components() {
        let s = ActixStrategy;
        assert!(s.test_imports().contains("use actix_web"));
        assert!(s
            .test_imports()
            .contains("use jsonschema::{Draft, JSONSchema};"));
    }
}
