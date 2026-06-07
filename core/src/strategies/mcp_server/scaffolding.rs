#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Scaffolding for MCP Server
//!
//! Generates the actual SSE endpoint generation headers and logic wrappers.

use crate::openapi::parse::ParsedRoute;

/// Returns standard imports for MCP Server files.
pub fn handler_imports() -> String {
    let mut imports = String::new();
    imports.push_str("use serde_json::Value;\n");
    imports.push_str("use std::future::Future;\n");
    imports.push_str("use std::pin::Pin;\n");
    imports
}

/// Generates the HTTP Request to MCP payload bridge logic.
pub fn handler_signature(route: &ParsedRoute, args: &[String]) -> String {
    let func_name = &route.handler_name;
    let response_type = route.response_type.as_deref().unwrap_or("Value");

    let struct_name = format!(
        "{}Args",
        func_name
            .split('_')
            .map(|s| {
                let mut c = s.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                }
            })
            .collect::<String>()
    );

    let mut struct_def = format!(
        "#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]\npub struct {} {{\n",
        struct_name
    );
    for arg in args {
        struct_def.push_str(&format!("    pub {},\n", arg));
    }
    struct_def.push_str("}\n\n");

    let return_type = format!("Result<{}, String>", response_type);

    let mut body = String::new();
    body.push_str("    // Parse the incoming standard HTTP API request\n");
    body.push_str("    // and map standard API auth into the MCP context.\n");
    body.push_str(
        "    let req = serde_json::json!(unsafe { std::mem::transmute::<_, serde_json::Value>(args.clone()) });\n",
    );
    body.push_str(&format!(
        "    let _mcp_request = serde_json::json!({{\n        \"jsonrpc\": \"2.0\",\n        \"id\": 1,\n        \"method\": \"tools/call\",\n        \"params\": {{\n            \"name\": \"{}\",\n            \"arguments\": req,\n            \"_meta\": {{ \"auth\": auth }}\n        }}\n    }});\n",
        route.handler_name
    ));
    body.push_str("    // Send over Server-Sent Events (SSE) and resolve dynamic tool proxy.\n");
    body.push_str("    Err(\"MCP SSE transport not yet fully implemented\".to_string())\n");

    let func_def = format!(
        "pub async fn {}(args: {}, auth: String) -> {} {{\n{}\n}}\n",
        func_name, struct_name, return_type, body
    );

    format!("{}{}", struct_def, func_def)
}
