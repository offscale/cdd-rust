#![deny(missing_docs)]

//! # Scaffolding for MCP Client
//!
//! Generates strongly typed MCP Tool definitions and JSON-RPC 2.0 `CallToolRequest`
//! client adapters using `cdd_core::mcp::models`.

use crate::openapi::parse::ParsedRoute;

/// Returns standard imports for MCP Client files.
///
/// # Returns
/// The Rust `use` statements required by generated MCP Client modules.
pub fn handler_imports() -> String {
    let mut imports = String::new();
    imports.push_str("use cdd_core::error::AppError;\n");
    imports.push_str("use cdd_core::mcp::models::{CallToolRequestParams, CallToolResult, JsonRpcRequest, RequestId, Tool, ToolInputSchema};
");
    imports.push_str(
        "use serde_json::Value;
",
    );
    imports.push_str(
        "use std::sync::Arc;
",
    );
    imports.push_str(
        "use reqwest::Client;
",
    );
    imports
}

/// Generates a strongly typed MCP [`crate::mcp::models::Tool`] constructor function and
/// an async `tools/call` client invocation function for the given [`ParsedRoute`].
///
/// # Arguments
/// * `route` - The parsed OpenAPI route definition.
/// * `args` - Extracted `name: Type` parameter declarations.
///
/// # Returns
/// Generated Rust struct, tool metadata helper, and async invocation function.
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

    let tool_fn = format!(
        r#"/// Returns the strongly typed MCP [`Tool`] metadata for `{func_name}`.
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

"#,
        func_name = func_name,
        desc_expr = desc_expr
    );

    let func_def = format!(
        r#"/// Builds and dispatches a strongly typed MCP `tools/call` JSON-RPC 2.0 request for `{func_name}`.
pub async fn {func_name}(
    args: {struct_name},
    client: Arc<Client>,
    base_url: &str,
) -> Result<CallToolResult, AppError> {{
    let args_val = serde_json::to_value(&args)?;
    let call_params = CallToolRequestParams {{
        name: "{func_name}".to_string(),
        arguments: Some(args_val),
    }};
    let rpc_req = JsonRpcRequest {{
        jsonrpc: "2.0".to_string(),
        id: RequestId::Number(1),
        method: "tools/call".to_string(),
        params: Some(serde_json::to_value(&call_params)?),
    }};
    let resp = client
        .post(base_url)
        .json(&rpc_req)
        .send()
        .await
        .map_err(|e| AppError::General(e.to_string()))?;
    resp.json::<CallToolResult>()
        .await
        .map_err(|e| AppError::General(e.to_string()))
}}
"#,
        func_name = func_name,
        struct_name = struct_name
    );

    format!("{}{}{}", struct_def, tool_fn, func_def)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::RouteKind;
    use ra_ap_edition::Edition;
    use ra_ap_syntax::SourceFile;
    use std::collections::BTreeMap;

    #[test]
    fn test_mcp_client_scaffolding_ast() {
        let route = ParsedRoute {
            path: "/pets".into(),
            summary: Some("List pets".into()),
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
            handler_name: "list_pets".into(),
            operation_id: None,
            params: vec![],
            path_params: vec![],
            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: Some("Vec<Pet>".into()),
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
            handler_signature(&route, &["limit: i32".to_string()])
        );
        let parse = SourceFile::parse(&code, Edition::Edition2021);
        assert!(
            parse.errors().is_empty(),
            "AST errors: {:?}",
            parse.errors()
        );
        assert!(code.contains("list_pets_tool_definition() -> Tool"));
        assert!(code.contains("CallToolRequestParams"));
        assert!(code.contains("Result<CallToolResult, AppError>"));
    }
}
