#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Route Registration for MCP Server
//!
//! Generates the dynamic API to Tool proxy registration endpoints.

use crate::openapi::parse::ParsedRoute;

/// Generates the SSE endpoint and tool proxy mapping
pub fn route_registration_statement(route: &ParsedRoute, handler_full_path: &str) -> String {
    format!(
        "            // MCP AI Gateway Proxy Registration
            registry.register_tool(\"{}\", Box::new({}));",
        route.handler_name, handler_full_path
    )
}
