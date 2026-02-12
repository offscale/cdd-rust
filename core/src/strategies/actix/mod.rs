#![deny(missing_docs)]

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

use crate::oas::models::{ParsedLink, ResponseHeader, SecurityRequirement};
use crate::oas::ParsedRoute;
use crate::strategies::BackendStrategy;

/// Strategy for generating Actix Web compatible code.
pub struct ActixStrategy;

impl BackendStrategy for ActixStrategy {
    // --- Scaffolding ---

    fn handler_imports(&self) -> String {
        scaffolding::handler_imports()
    }

    fn handler_signature(
        &self,
        func_name: &str,
        args: &[String],
        response_type: Option<&str>,
        response_headers: &[ResponseHeader],
        response_links: Option<&[ParsedLink]>,
    ) -> String {
        scaffolding::handler_signature(
            func_name,
            args,
            response_type,
            response_headers,
            response_links,
        )
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

    fn query_string_extractor(&self, inner_type: &str) -> String {
        extractors::query_string_extractor(inner_type)
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

    fn security_extractor(&self, requirements: &[SecurityRequirement]) -> String {
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

    fn test_body_setup_code(&self, body: &crate::oas::RequestBodyDefinition) -> String {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::{LinkParamValue, ParsedLink, RouteKind, RuntimeExpression};
    use crate::oas::ParsedRoute;
    use std::collections::HashMap;

    #[test]
    fn test_actix_handler_imports() {
        let s = ActixStrategy;
        let imports = s.handler_imports();
        assert!(imports.contains("use actix_web"));
        assert!(imports.contains("use uuid::Uuid"));
    }

    #[test]
    fn test_actix_handler_signature_with_links_generated() {
        let s = ActixStrategy;
        let mut params = HashMap::new();
        params.insert(
            "id".to_string(),
            LinkParamValue::Expression(RuntimeExpression::new("$request.path.id")),
        );

        let links = vec![ParsedLink {
            name: "Self".to_string(),
            description: None,
            operation_id: None,
            operation_ref: Some("/users/{id}".to_string()),
            parameters: params,
            request_body: None,
            server_url: None,
        }];

        let code = s.handler_signature(
            "get_user",
            &["id: Uuid".to_string()],
            Some("User"),
            &[],
            Some(&links),
        );

        assert!(code.contains("-> actix_web::Result<HttpResponse>"));
    }

    #[test]
    fn test_actix_route_registration_custom_verb() {
        let s = ActixStrategy;
        let route = ParsedRoute {
            path: "/path".into(),
            summary: None,
            description: None,
            base_path: None,
            method: "QUERY".into(),
            handler_name: "query_handler".into(),
            params: vec![],
            request_body: None,
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            tags: vec![],
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
    }
}
