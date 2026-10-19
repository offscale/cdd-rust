#![deny(missing_docs)]

//! # Scaffolding for MCP Server
//!
//! Generates complete JSON-RPC 2.0 / SSE MCP Server tool handlers using
//! `cdd_core::mcp::models`.

use crate::openapi::parse::ParsedRoute;

/// Returns standard imports for MCP Server files.
///
/// # Returns
/// The Rust `use` statements required by generated MCP Server modules.
pub fn handler_imports() -> String {
    let mut imports = String::new();
    imports.push_str("use cdd_core::error::AppError;\n");
    imports.push_str(
        "use cdd_core::mcp::models::{CallToolResult, CallToolResultContent, JsonRpcResponse, RequestId, TextContent, Tool, ToolInputSchema};
",
    );
    imports.push_str(
        "use serde_json::Value;
",
    );
    imports.push_str(
        "use std::future::Future;
",
    );
    imports.push_str(
        "use std::pin::Pin;
",
    );
    imports
}

/// Generates the strongly typed MCP server tool handler and SSE event builder.
///
/// # Arguments
/// * `route` - The parsed OpenAPI route definition.
/// * `args` - Extracted `name: Type` parameter declarations.
///
/// # Returns
/// Generated Rust struct, tool definition, and async JSON-RPC / SSE handler functions.
pub fn handler_signature(route: &ParsedRoute, args: &[String]) -> String {
    let func_name = &route.handler_name;

    let struct_name = format!(
        "{}Args",
        func_name
            .split('_')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let mut c = s.chars();
                let first = c.next().into_iter().flat_map(char::to_uppercase);
                first.chain(c).collect::<String>()
            })
            .collect::<String>()
    );

    let mut struct_def = format!(
        "#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default, PartialEq)]
pub struct {} {{
",
        struct_name
    );
    for arg in args {
        struct_def.push_str(&format!(
            "    pub {},
",
            arg
        ));
    }
    struct_def.push_str(
        "}

",
    );

    let desc_expr = match &route.summary {
        Some(s) => format!("Some({:?}.to_string())", s),
        None => "None".to_string(),
    };

    let func_def = format!(
        r#"/// Returns the strongly typed MCP [`Tool`] schema for `{func_name}`.
pub fn {func_name}_tool_definition() -> Tool {{
    Tool {{
        name: "{func_name}".to_string(),
        description: {desc_expr},
        input_schema: ToolInputSchema {{
            schema_type: "object".to_string(),
            properties: None,
            required: None,
        }},
    }}
}}

/// Executes the `{func_name}` MCP server tool and formats the result as a JSON-RPC 2.0 SSE event.
pub async fn {func_name}(args: {struct_name}, auth: String) -> Result<String, AppError> {{
    let payload = serde_json::to_value(&args)?;
    let tool_result = CallToolResult {{
        content: vec![CallToolResultContent::Text(TextContent {{
            text: serde_json::json!({{
                "tool": "{func_name}",
                "auth": auth,
                "arguments": payload
            }})
            .to_string(),
        }})],
        is_error: Some(false),
        meta: None,
    }};
    let rpc_resp = JsonRpcResponse {{
        jsonrpc: "2.0".to_string(),
        id: RequestId::Number(1),
        result: Some(serde_json::to_value(&tool_result)?),
        error: None,
    }};
    let serialized = serde_json::to_string(&rpc_resp)?;
    Ok(format!("event: message
data: {{}}

", serialized))
}}
"#,
        func_name = func_name,
        struct_name = struct_name,
        desc_expr = desc_expr
    );

    format!("{}{}", struct_def, func_def)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::RouteKind;
    use ra_ap_edition::Edition;
    use ra_ap_syntax::SourceFile;
    use std::collections::BTreeMap;

    #[test]
    fn test_mcp_server_scaffolding_ast() {
        let route = ParsedRoute {
            path: "/pets".into(),
            summary: Some("Create pet".into()),
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
            response_type: Some("Pet".into()),
            response_status: Some("200".into()),
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

        let code = format!(
            "{}
{}",
            handler_imports(),
            handler_signature(&route, &["name: String".to_string()])
        );
        let parse = std::mem::ManuallyDrop::new(SourceFile::parse(&code, Edition::Edition2021));
        assert!(
            parse.errors().is_empty(),
            "AST errors: {:?}",
            parse.errors()
        );
        assert!(code.contains("create_pet_tool_definition() -> Tool"));
        assert!(code.contains("event: message"));
        assert!(code.contains("JsonRpcResponse"));
    }
}
