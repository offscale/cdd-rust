#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Reqwest Strategy Module
//!
//! Implementation of `BackendStrategy` for generating a pure Rust client library
//! using `reqwest` and `serde`.

pub mod extractors;
pub mod registration;
pub mod scaffolding;
pub mod testing;

use crate::openapi::parse::models::{ParsedLink, ResponseHeader};
use crate::openapi::parse::ParsedRoute;
use crate::strategies::BackendStrategy;

/// Strategy for generating Reqwest client SDK code.
pub struct ReqwestStrategy;

impl BackendStrategy for ReqwestStrategy {
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

    fn extra_handler_docs(&self, route: &ParsedRoute) -> String {
        format!(
            "/// @OAS_METHOD: {}\n/// @OAS_PATH: {}\n",
            route.method, route.path
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
}
