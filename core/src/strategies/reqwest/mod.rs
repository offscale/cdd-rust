#![deny(missing_docs)]

//! # Reqwest Strategy Module
//!
//! Implementation of [`CodegenStrategy`], [`HttpClientStrategy`], and [`BackendStrategy`]
//! for generating a pure Rust client library using `reqwest` and `serde`.

pub mod extractors;
pub mod registration;
pub mod scaffolding;
pub mod testing;

use crate::openapi::parse::ParsedRoute;
use crate::strategies::{BackendStrategy, CodegenStrategy, HttpClientStrategy, StrategyKind};

/// Strategy for generating Reqwest client SDK code.
pub struct ReqwestStrategy;

impl CodegenStrategy for ReqwestStrategy {
    fn kind(&self) -> StrategyKind {
        StrategyKind::HttpClient
    }

    fn module_imports(&self) -> String {
        scaffolding::handler_imports()
    }

    fn emit_handler(&self, route: &ParsedRoute, args: &[String]) -> String {
        scaffolding::handler_signature(route, args)
    }
}

impl HttpClientStrategy for ReqwestStrategy {
    fn client_operation_docs(&self, route: &ParsedRoute) -> String {
        format!(
            "/// @OAS_METHOD: {}
/// @OAS_PATH: {}
",
            route.method, route.path
        )
    }

    fn client_mock_test(&self, route: &ParsedRoute, crate_name: &str) -> String {
        testing::generate_custom_test(route, crate_name)
    }
}

impl BackendStrategy for ReqwestStrategy {
    fn extra_handler_docs(&self, route: &ParsedRoute) -> String {
        self.client_operation_docs(route)
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

    fn generate_custom_test(&self, route: &ParsedRoute, app_factory: &str) -> Option<String> {
        Some(self.client_mock_test(route, app_factory))
    }
}
