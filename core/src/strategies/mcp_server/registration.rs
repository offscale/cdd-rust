#![deny(missing_docs)]

//! # Route Registration for MCP Server
//!
//! Generates the dynamic API to Tool proxy registration endpoints.

use crate::openapi::parse::ParsedRoute;

/// Generates the SSE endpoint and tool proxy mapping statement.
///
/// # Arguments
/// * `route` - The parsed OpenAPI route definition.
/// * `handler_full_path` - Full path to the generated tool handler.
///
/// # Returns
/// Rust statement registering the tool in `registry`.
pub fn route_registration_statement(route: &ParsedRoute, handler_full_path: &str) -> String {
    format!(
        r#"            // MCP AI Gateway Proxy Registration
            registry.register_tool("{}", Box::new({}));"#,
        route.handler_name, handler_full_path
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::RouteKind;
    use ra_ap_edition::Edition;
    use ra_ap_syntax::SourceFile;
    use std::collections::BTreeMap;

    #[test]
    fn test_mcp_server_registration_ast() {
        let route = ParsedRoute {
            path: "/pets".into(),
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
            handler_name: "create_pet".into(),
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
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let stmt = route_registration_statement(&route, "handlers::pets::create_pet");
        let wrapped = format!(
            "fn reg(registry: &mut ToolRegistry) {{
{}
}}",
            stmt
        );
        let parse = std::mem::ManuallyDrop::new(SourceFile::parse(&wrapped, Edition::Edition2021));
        assert!(
            parse.errors().is_empty(),
            "AST errors: {:?}",
            parse.errors()
        );
        assert!(stmt.contains("registry.register_tool"));
    }
}
