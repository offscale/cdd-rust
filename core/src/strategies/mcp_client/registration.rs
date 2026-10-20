#![deny(missing_docs)]

//! # Route Registration for MCP Client
//!
//! Generates strongly typed MCP `Tool` registration statements using `cdd_core::mcp::models`.

use crate::openapi::parse::ParsedRoute;

/// Generates the strongly typed `cdd_core::mcp::models::Tool` registration statement.
///
/// # Arguments
/// * `route` - The parsed OpenAPI route definition.
/// * `handler_full_path` - The module path to the generated tool handler.
///
/// # Returns
/// A Rust statement constructing a [`crate::mcp::models::Tool`] entry.
pub fn route_registration_statement(route: &ParsedRoute, handler_full_path: &str) -> String {
    let desc_expr = match &route.summary {
        Some(s) => format!("Some({:?}.to_string())", s),
        None => "None".to_string(),
    };
    format!(
        r#"            // Handler: {}
            tools.push(cdd_core::mcp::models::Tool {{
                name: "{}".to_string(),
                description: {},
                input_schema: cdd_core::mcp::models::ToolInputSchema {{
                    schema_type: "object".to_string(),
                    properties: None,
                    required: None,
                }},
            }});"#,
        handler_full_path, route.handler_name, desc_expr
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
    fn test_mcp_client_registration_ast() {
        let route = ParsedRoute {
            path: "/pets".into(),
            summary: Some("Get pets".into()),
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
            handler_name: "get_pets".into(),
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

        let stmt = route_registration_statement(&route, "handlers::pets::get_pets");
        let wrapped = format!(
            "fn register(tools: &mut Vec<cdd_core::mcp::models::Tool>) {{
{}
}}",
            stmt
        );
        let parse = SourceFile::parse(&wrapped, Edition::Edition2021);
        assert!(
            parse.errors().is_empty(),
            "AST errors: {:?}",
            parse.errors()
        );
        assert!(stmt.contains("cdd_core::mcp::models::Tool"));
        assert!(stmt.contains("get_pets"));
    }
}
