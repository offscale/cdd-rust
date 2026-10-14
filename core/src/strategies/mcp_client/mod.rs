#![deny(missing_docs)]

//! # MCP Client Strategy Module
//!
//! Implementation of [`CodegenStrategy`], [`McpStrategy`], and [`BackendStrategy`]
//! for generating native MCP tool adapters.

pub mod extractors;
pub mod registration;
pub mod scaffolding;
pub mod testing;

use crate::openapi::parse::ParsedRoute;
use crate::strategies::{BackendStrategy, CodegenStrategy, McpStrategy, StrategyKind};

/// Strategy for generating MCP programmatic tool bindings.
pub struct McpClientStrategy;

impl CodegenStrategy for McpClientStrategy {
    fn kind(&self) -> StrategyKind {
        StrategyKind::Mcp
    }

    fn module_imports(&self) -> String {
        scaffolding::handler_imports()
    }

    fn emit_handler(&self, route: &ParsedRoute, args: &[String]) -> String {
        scaffolding::handler_signature(route, args)
    }
}

impl McpStrategy for McpClientStrategy {
    fn mcp_tool_registration(&self, route: &ParsedRoute, handler_full_path: &str) -> String {
        registration::route_registration_statement(route, handler_full_path)
    }
}

impl BackendStrategy for McpClientStrategy {
    fn extra_handler_docs(&self, route: &ParsedRoute) -> String {
        format!(
            "/// @MCP_TOOL: {}
",
            route.handler_name
        )
    }

    fn route_registration_statement(&self, route: &ParsedRoute, handler_full_path: &str) -> String {
        self.mcp_tool_registration(route, handler_full_path)
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
