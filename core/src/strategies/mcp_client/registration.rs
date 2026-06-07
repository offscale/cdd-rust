#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Route Registration for MCP Client
//!
//! Generates the MCP tool list registration.

use crate::openapi::parse::ParsedRoute;

/// Generates the Native MCP Tool Adapter mapping logic
pub fn route_registration_statement(route: &ParsedRoute, handler_full_path: &str) -> String {
    format!(
        "            {{
                \"name\": \"{}\",
                \"description\": \"{}\",
                \"handler\": {}
            }},",
        route.handler_name,
        route.summary.clone().unwrap_or_default(),
        handler_full_path
    )
}
