#![deny(missing_docs)]

//! MCP Server Implementation & Unified Tool Dispatch
//!
//! Provides the Model Context Protocol (MCP) STDIO server using the strongly typed
//! models in `cdd_core::mcp::models` as well as a unified tool registry and execution
//! dispatcher shared with `serve_json_rpc`.

use crate::from_arazzo::{from_arazzo, FromArazzoConfig, FromArazzoSubcommand};
use crate::from_openapi::{
    from_openapi, FromOpenApiConfig, FromOpenApiSubcommand, ServerFramework,
};
use crate::sync::{sync, SyncConfig};
use crate::to_arazzo::{to_arazzo, ToArazzoConfig};
use crate::to_docs_json::{generate_docs_json_str, to_docs_json, ToDocsJsonArgs, ToDocsJsonConfig};
use crate::to_openapi::{to_openapi, ToOpenApiConfig};
use crate::TargetMode;
use cdd_core::error::{AppError, AppResult};
use cdd_core::mcp::models::{
    CallToolRequestParams, CallToolResult, CallToolResultContent, EmptyResult, Implementation,
    InitializeResult, JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
    ListToolsResult, RequestId, ServerCapabilities, ServerToolsCapability, TextContent, Tool,
    ToolInputSchema,
};
use clap::Args;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::str::FromStr;

/// Arguments for the MCP server command.
#[derive(Args, Debug, Default, Clone)]
pub struct McpArgs {
    /// Optional inline JSON-RPC payload to process before exiting.
    #[clap(long, default_value = "")]
    pub inline: String,
}

/// Executes the `mcp` command, running an MCP server over STDIO.
///
/// # Arguments
/// * `args` - The command-line arguments for the `mcp` command.
///
/// # Returns
/// `Ok(())` when the STDIO loop terminates cleanly.
///
/// # Errors
/// Returns [`AppError`] if writing to stdout fails.
pub fn run_mcp(args: &McpArgs) -> AppResult<()> {
    serve_mcp(args)
}

/// Alias for [`run_mcp`] used by CLI command dispatch.
///
/// # Arguments
/// * `args` - The command-line arguments for the `mcp` command.
///
/// # Returns
/// `Ok(())` when the STDIO loop terminates cleanly.
///
/// # Errors
/// Returns [`AppError`] if writing to stdout fails.
pub fn run_mcp_server(args: &McpArgs) -> AppResult<()> {
    run_mcp(args)
}

/// Programmatic entry point for the MCP server.
///
/// # Arguments
/// * `args` - Configuration arguments for the MCP server.
///
/// # Returns
/// `Ok(())` when the STDIO loop terminates cleanly.
///
/// # Errors
/// Returns [`AppError`] if writing to stdout fails.
pub fn serve_mcp(args: &McpArgs) -> AppResult<()> {
    let mut reader = io::Cursor::new(args.inline.as_bytes());
    let mut stdout = io::stdout();
    serve_mcp_inner(&mut reader, &mut stdout)
}

/// Inner loop for the MCP server, taking `BufRead` and `Write` trait objects for testability.
///
/// # Arguments
/// * `reader` - Input stream carrying newline-delimited JSON-RPC messages.
/// * `writer` - Output stream for writing JSON-RPC responses.
///
/// # Returns
/// `Ok(())` when EOF is reached or a read error occurs.
///
/// # Errors
/// Returns [`AppError`] if writing or flushing `writer` fails.
pub fn serve_mcp_inner(reader: &mut dyn BufRead, writer: &mut dyn Write) -> AppResult<()> {
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = match reader.read_line(&mut line) {
            Ok(b) => b,
            Err(_) => break,
        };
        if bytes_read == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(resp_str) = handle_request(trimmed) {
            writeln!(writer, "{}", resp_str)?;
            writer.flush()?;
        }
    }

    Ok(())
}

/// Returns the canonical list of strongly typed MCP [`Tool`] definitions supported by `cdd-cli`.
///
/// # Returns
/// A vector of [`Tool`] schemas for `from_openapi`, `to_openapi`, `to_docs_json`, and `sync`.
pub fn supported_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "from_openapi".to_string(),
            description: Some(
                "Generate Rust code (SDK, CLI, or Server) from an OpenAPI specification"
                    .to_string(),
            ),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "subcommand": {
                        "type": "string",
                        "enum": ["to_sdk", "to_sdk_cli", "to_server"],
                        "description": "Generation target"
                    },
                    "input": { "type": "string", "description": "Input OpenAPI spec path or URL" },
                    "input_dir": { "type": "string", "description": "Input directory containing specs" },
                    "output": { "type": "string", "description": "Output directory path" },
                    "framework": { "type": "string", "enum": ["actix-web", "axum"], "description": "Server framework" },
                    "no_github_actions": { "type": "boolean" },
                    "no_installable_package": { "type": "boolean" }
                })),
                required: None,
            },
        },
        Tool {
            name: "to_openapi".to_string(),
            description: Some(
                "Generate an OpenAPI specification from existing Rust source code".to_string(),
            ),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "input": { "type": "string", "description": "Input Rust file or directory" },
                    "output": { "type": "string", "description": "Output OpenAPI file path" }
                })),
                required: Some(vec!["input".to_string()]),
            },
        },
        Tool {
            name: "to_docs_json".to_string(),
            description: Some(
                "Generate documentation JSON snippets from an OpenAPI specification".to_string(),
            ),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "input": { "type": "string", "description": "Input OpenAPI file path or URL" },
                    "output": { "type": "string", "description": "Optional output file path" },
                    "no_imports": { "type": "boolean" },
                    "no_wrapping": { "type": "boolean" }
                })),
                required: Some(vec!["input".to_string()]),
            },
        },
        Tool {
            name: "sync".to_string(),
            description: Some("Synchronize OpenAPI specification and Rust code".to_string()),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "dir": { "type": "string", "description": "Project directory to sync" }
                })),
                required: Some(vec!["dir".to_string()]),
            },
        },
        Tool {
            name: "from_arazzo".to_string(),
            description: Some(
                "Generate Rust code (SDK, CLI, or Server) from an Arazzo 1.1.0 specification"
                    .to_string(),
            ),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "subcommand": {
                        "type": "string",
                        "enum": ["to_sdk", "to_sdk_cli", "to_server"],
                        "description": "Generation target"
                    },
                    "input": { "type": "string", "description": "Input Arazzo spec path or URL" },
                    "output": { "type": "string", "description": "Output directory path" },
                    "framework": { "type": "string", "enum": ["actix-web", "axum"] }
                })),
                required: None,
            },
        },
        Tool {
            name: "to_arazzo".to_string(),
            description: Some(
                "Extract an Arazzo 1.1.0 workflow specification from Rust source code".to_string(),
            ),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({
                    "input": { "type": "string", "description": "Input Rust file or directory" },
                    "output": { "type": "string", "description": "Output Arazzo file path" }
                })),
                required: Some(vec!["input".to_string()]),
            },
        },
    ]
}

/// Dispatches a CLI tool command by name and JSON arguments, returning a human-readable
/// or JSON payload string. Shared between MCP `tools/call` and `serve_json_rpc`.
///
/// # Arguments
/// * `tool_name` - Name of the tool (`"from_openapi"`, `"to_openapi"`, `"to_docs_json"`, `"sync"`).
/// * `args` - JSON object containing the tool parameters.
/// * `default_subcommand` - Fallback subcommand for `from_openapi` when `subcommand` is omitted.
///
/// # Returns
/// A success message or generated JSON output string.
///
/// # Errors
/// Returns [`AppError`] if required arguments are missing, invalid, or execution fails.
pub fn dispatch_tool_command(
    tool_name: &str,
    args: &Value,
    default_subcommand: Option<FromOpenApiSubcommand>,
) -> AppResult<String> {
    match tool_name {
        "from_openapi" | "to_sdk" | "to_sdk_cli" | "to_server" => {
            let subcommand = if let Some(sub_str) = args.get("subcommand").and_then(Value::as_str) {
                FromOpenApiSubcommand::from_str(sub_str)?
            } else if matches!(tool_name, "to_sdk" | "to_sdk_cli" | "to_server") {
                FromOpenApiSubcommand::from_str(tool_name)?
            } else {
                default_subcommand.unwrap_or(FromOpenApiSubcommand::ToSdk)
            };

            let input = args.get("input").and_then(Value::as_str).map(PathBuf::from);
            let input_dir = args
                .get("input_dir")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            let output_dir = args
                .get("output")
                .or_else(|| args.get("output_dir"))
                .and_then(Value::as_str)
                .map(PathBuf::from);
            let no_github_actions = args
                .get("no_github_actions")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let no_installable_package = args
                .get("no_installable_package")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let tests = args.get("tests").and_then(Value::as_bool).unwrap_or(false);
            let mcp = args.get("mcp").and_then(Value::as_bool).unwrap_or(false);
            let framework_str = args
                .get("framework")
                .and_then(Value::as_str)
                .unwrap_or("actix-web");

            let framework = match framework_str {
                "axum" => ServerFramework::Axum,
                _ => ServerFramework::ActixWeb,
            };

            let github_org = args
                .get("github_org")
                .and_then(Value::as_str)
                .map(str::to_string);
            let repo_name = args
                .get("repo_name")
                .and_then(Value::as_str)
                .map(str::to_string);
            let monorepo = args
                .get("monorepo")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let config = FromOpenApiConfig {
                subcommand,
                input,
                input_dir,
                output_dir,
                no_github_actions,
                no_installable_package,
                tests,
                mcp,
                framework,
                github_org,
                repo_name,
                monorepo,
            };

            from_openapi(&config)?;
            Ok(format!("Successfully executed {:?}", subcommand))
        }
        "to_openapi" => {
            let input_str = args
                .get("input")
                .and_then(Value::as_str)
                .ok_or(AppError::MissingArgument { argument: "input" })?;
            let output_str = args
                .get("output")
                .and_then(Value::as_str)
                .unwrap_or("openapi.json");

            let config = ToOpenApiConfig {
                input: PathBuf::from(input_str),
                output: PathBuf::from(output_str),
                target: TargetMode::ServerActix,
                title: None,
                version: None,
            };
            to_openapi(&config)?;
            Ok(format!(
                "Successfully generated OpenAPI spec at {}",
                output_str
            ))
        }
        "to_docs_json" => {
            let input_str = args
                .get("input")
                .and_then(Value::as_str)
                .ok_or(AppError::MissingArgument { argument: "input" })?;
            let no_imports = args
                .get("no_imports")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let no_wrapping = args
                .get("no_wrapping")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let output_opt = args
                .get("output")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);

            if output_opt.is_some() {
                let config = ToDocsJsonConfig {
                    input: input_str.to_string(),
                    no_imports,
                    no_wrapping,
                    output: output_opt.clone(),
                };
                to_docs_json(&config)?;
                Ok(format!(
                    "Successfully generated docs JSON for {}",
                    input_str
                ))
            } else {
                let cli_args = ToDocsJsonArgs {
                    input: input_str.to_string(),
                    no_imports,
                    no_wrapping,
                    output: None,
                };
                generate_docs_json_str(&cli_args)
            }
        }
        "sync" => {
            let dir_str = args
                .get("dir")
                .or_else(|| args.get("input"))
                .and_then(Value::as_str)
                .ok_or(AppError::MissingArgument { argument: "dir" })?;
            let truth_str = args.get("truth").and_then(Value::as_str).unwrap_or("code");
            let output_str = args
                .get("output")
                .and_then(Value::as_str)
                .unwrap_or(dir_str);

            let config = SyncConfig {
                truth: truth_str.to_string(),
                input: PathBuf::from(dir_str),
                output: PathBuf::from(output_str),
                no_gen: args.get("no_gen").and_then(Value::as_bool).unwrap_or(false),
                force_type: vec![],
            };
            let mapper = crate::generator::DieselMapper;
            sync(&config, &mapper)?;
            Ok(format!("Successfully synced directory {}", dir_str))
        }
        "from_arazzo" => {
            let subcommand = if let Some(sub_str) = args.get("subcommand").and_then(Value::as_str) {
                FromArazzoSubcommand::from_str(sub_str)?
            } else {
                FromArazzoSubcommand::ToSdk
            };
            let input = args.get("input").and_then(Value::as_str).map(PathBuf::from);
            let input_dir = args
                .get("input_dir")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            let output_dir = args
                .get("output")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            let framework = match args
                .get("framework")
                .and_then(Value::as_str)
                .unwrap_or("actix-web")
            {
                "axum" => ServerFramework::Axum,
                _ => ServerFramework::ActixWeb,
            };
            let config = FromArazzoConfig {
                subcommand,
                input,
                input_dir,
                output_dir,
                openapi_dir: None,
                framework,
                no_github_actions: args
                    .get("no_github_actions")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                no_installable_package: args
                    .get("no_installable_package")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                tests: args.get("tests").and_then(Value::as_bool).unwrap_or(false),
                mcp: args.get("mcp").and_then(Value::as_bool).unwrap_or(false),
                github_org: args
                    .get("github_org")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                repo_name: args
                    .get("repo_name")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                monorepo: args
                    .get("monorepo")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            };
            from_arazzo(&config)?;
            Ok(format!(
                "Successfully executed from_arazzo {:?}",
                subcommand
            ))
        }
        "to_arazzo" => {
            let input_str = args
                .get("input")
                .and_then(Value::as_str)
                .ok_or(AppError::MissingArgument { argument: "input" })?;
            let output_str = args
                .get("output")
                .and_then(Value::as_str)
                .unwrap_or("arazzo.yaml");
            let config = ToArazzoConfig {
                input: PathBuf::from(input_str),
                output: PathBuf::from(output_str),
                title: args
                    .get("title")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                version: args
                    .get("version")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                source_url: args
                    .get("source_url")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
            };
            to_arazzo(&config)?;
            Ok(format!(
                "Successfully generated Arazzo spec at {}",
                output_str
            ))
        }
        _ => Err(AppError::ToolNotFound {
            tool: tool_name.to_string(),
        }),
    }
}

/// Builds a serialized [`JsonRpcResponse`] with a strongly typed result payload.
///
/// # Arguments
/// * `id` - Input `id` parameter.
/// * `result` - Input `result` parameter.
///
/// # Returns
/// Resulting `Option<String>` value.
fn ok_response<T: serde::Serialize>(id: RequestId, result: &T) -> Option<String> {
    serde_json::to_value(result).ok().and_then(|val| {
        let resp = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(val),
            error: None,
        };
        serde_json::to_string(&resp).ok()
    })
}

/// Builds a serialized [`JsonRpcResponse`] with a [`JsonRpcError`].
///
/// # Arguments
/// * `id` - Input `id` parameter.
/// * `code` - Input `code` parameter.
/// * `message` - Input `message` parameter.
///
/// # Returns
/// Resulting `Option<String>` value.
fn err_response(id: RequestId, code: i32, message: impl Into<String>) -> Option<String> {
    let resp = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.into(),
            data: None,
        }),
    };
    serde_json::to_string(&resp).ok()
}

/// Processes a single JSON-RPC 2.0 request or notification line using strongly typed MCP models.
///
/// # Arguments
/// * `input` - Raw JSON line read from the client.
///
/// # Returns
/// `Some(json_string)` for requests or parse errors, or `None` for notifications.
pub fn handle_request(input: &str) -> Option<String> {
    let raw_value: Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(_) => return err_response(RequestId::Null, -32700, "Parse error"),
    };

    // Distinguish notifications (no `id` field) from requests
    if raw_value.get("id").is_none() {
        if serde_json::from_value::<JsonRpcNotification>(raw_value).is_ok() {
            return None;
        }
        return err_response(RequestId::Null, -32600, "Invalid Request");
    }

    let req: JsonRpcRequest = match serde_json::from_value(raw_value) {
        Ok(r) => r,
        Err(_) => return err_response(RequestId::Null, -32600, "Invalid Request"),
    };

    if req.jsonrpc != "2.0" {
        return err_response(req.id, -32600, "Invalid Request: jsonrpc must be '2.0'");
    }

    match req.method.as_str() {
        "initialize" => {
            let init_res = InitializeResult {
                protocol_version: "2024-11-05".to_string(),
                capabilities: ServerCapabilities {
                    experimental: None,
                    logging: None,
                    prompts: None,
                    resources: None,
                    tools: Some(ServerToolsCapability { list_changed: None }),
                },
                server_info: Implementation {
                    name: "cdd-rust-mcp".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
                instructions: None,
                meta: None,
            };
            ok_response(req.id, &init_res)
        }
        "ping" => ok_response(req.id, &EmptyResult {}),
        "tools/list" => {
            let list_res = ListToolsResult {
                tools: supported_tools(),
                next_cursor: None,
                meta: None,
            };
            ok_response(req.id, &list_res)
        }
        "tools/call" => {
            let params_val = req.params.unwrap_or_else(|| json!({}));
            let call_params: CallToolRequestParams = match serde_json::from_value(params_val) {
                Ok(p) => p,
                Err(e) => {
                    return err_response(req.id, -32602, format!("Invalid params: {}", e));
                }
            };

            let args_val = call_params.arguments.unwrap_or_else(|| json!({}));
            match dispatch_tool_command(&call_params.name, &args_val, None) {
                Ok(msg) => {
                    let result = CallToolResult {
                        content: vec![CallToolResultContent::Text(TextContent { text: msg })],
                        is_error: Some(false),
                        meta: None,
                    };
                    ok_response(req.id, &result)
                }
                Err(e @ AppError::ToolNotFound { .. }) => {
                    err_response(req.id, -32601, e.to_string())
                }
                Err(e) => {
                    let result = CallToolResult {
                        content: vec![CallToolResultContent::Text(TextContent {
                            text: format!("Error executing {}: {}", call_params.name, e),
                        })],
                        is_error: Some(true),
                        meta: None,
                    };
                    ok_response(req.id, &result)
                }
            }
        }
        _ => err_response(req.id, -32601, "Method not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct ErrorReader;

    impl io::Read for ErrorReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("read error"))
        }
    }

    impl io::BufRead for ErrorReader {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            Err(io::Error::other("read error"))
        }
        fn consume(&mut self, _amt: usize) {}
    }

    #[test]
    fn test_mcp_initialize_and_ping() {
        let req = r#"{"jsonrpc": "2.0", "id": 1, "method": "initialize"}"#;
        let resp_str = handle_request(req).expect("Failed to handle request");
        let resp: JsonRpcResponse = serde_json::from_str(&resp_str).expect("Valid response");
        assert_eq!(resp.id, RequestId::Number(1));
        let init: InitializeResult =
            serde_json::from_value(resp.result.expect("Result present")).expect("Valid init");
        assert_eq!(init.server_info.name, "cdd-rust-mcp");

        let ping_req = r#"{"jsonrpc": "2.0", "id": "p1", "method": "ping"}"#;
        let ping_str = handle_request(ping_req).expect("Ping response");
        let ping_resp: JsonRpcResponse = serde_json::from_str(&ping_str).expect("Valid ping");
        assert_eq!(ping_resp.id, RequestId::String("p1".to_string()));
    }

    #[test]
    fn test_mcp_tools_list() {
        let req = r#"{"jsonrpc": "2.0", "id": 2, "method": "tools/list"}"#;
        let resp_str = handle_request(req).expect("Failed to handle request");
        let resp: JsonRpcResponse = serde_json::from_str(&resp_str).expect("Valid response");
        let list: ListToolsResult =
            serde_json::from_value(resp.result.expect("Result present")).expect("Valid list");
        assert_eq!(list.tools.len(), 6);
        assert!(list.tools.iter().any(|t| t.name == "from_openapi"));
        assert!(list.tools.iter().any(|t| t.name == "to_docs_json"));
        assert!(list.tools.iter().any(|t| t.name == "from_arazzo"));
        assert!(list.tools.iter().any(|t| t.name == "to_arazzo"));
    }

    #[test]
    fn test_mcp_notification_and_invalid_envelopes() {
        let notif = r#"{"jsonrpc": "2.0", "method": "notifications/initialized"}"#;
        assert!(handle_request(notif).is_none());

        let invalid_notif = r#"{"jsonrpc": "2.0"}"#;
        let resp_inv = handle_request(invalid_notif).expect("Invalid notification error");
        assert!(resp_inv.contains("-32600"));

        let bad_ver = r#"{"jsonrpc": "1.0", "id": 1, "method": "ping"}"#;
        let resp_ver = handle_request(bad_ver).expect("Bad version error");
        assert!(resp_ver.contains("-32600"));

        let bad_id = r#"{"jsonrpc": "2.0", "id": [1, 2], "method": "ping"}"#;
        let resp_id = handle_request(bad_id).expect("Bad id error");
        assert!(resp_id.contains("-32600"));
    }

    #[test]
    fn test_mcp_parse_error() {
        let req = r#"invalid json"#;
        let resp = handle_request(req).expect("Failed to handle request");
        assert!(resp.contains("-32700"));
    }

    #[test]
    fn test_mcp_method_not_found() {
        let req = r#"{"jsonrpc": "2.0", "id": 3, "method": "unknown"}"#;
        let resp = handle_request(req).expect("Failed to handle request");
        assert!(resp.contains("-32601"));
    }

    #[test]
    fn test_mcp_tools_call_unknown_and_invalid_params() {
        let req = r#"{"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "unknown_tool"}}"#;
        let resp = handle_request(req).expect("Failed to handle request");
        assert!(resp.contains("Tool not found: unknown_tool"));

        let bad_params =
            r#"{"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": "not_an_object"}"#;
        let resp_bad = handle_request(bad_params).expect("Invalid params response");
        assert!(resp_bad.contains("-32602"));

        let no_params = r#"{"jsonrpc": "2.0", "id": 6, "method": "tools/call"}"#;
        let resp_no_params = handle_request(no_params).expect("Missing params response");
        assert!(resp_no_params.contains("-32602"));
    }

    #[test]
    fn test_mcp_tools_call_from_openapi() {
        let dir = tempfile::tempdir().expect("Failed to create temporary directory");
        let spec_path = dir.path().join("openapi.json");
        std::fs::write(
            &spec_path,
            r#"{"openapi":"3.0.0","info":{"title":"T","version":"1"},"paths":{}}"#,
        )
        .expect("Failed to write to file");
        let out_dir = dir.path().join("out");

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "from_openapi",
                "arguments": {
                    "input": spec_path.to_str().expect("Failed to convert path to str"),
                    "output": out_dir.to_str().expect("Failed to convert path to str")
                }
            }
        })
        .to_string();

        let resp = handle_request(&req).expect("Failed to handle request");
        assert!(resp.contains("Successfully executed ToSdk"));

        let direct_sdk = dispatch_tool_command(
            "to_sdk",
            &serde_json::json!({
                "input": spec_path.to_str().expect("path"),
                "output": out_dir.join("direct").to_str().expect("path")
            }),
            None,
        )
        .expect("direct to_sdk");
        assert!(direct_sdk.contains("ToSdk"));

        let req_axum = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 51,
            "method": "tools/call",
            "params": {
                "name": "from_openapi",
                "arguments": {
                    "subcommand": "to_server",
                    "framework": "axum",
                    "input": spec_path.to_str().expect("Failed to convert path to str"),
                    "output": out_dir.join("axum").to_str().expect("Failed to convert path to str")
                }
            }
        })
        .to_string();
        let resp_axum = handle_request(&req_axum).expect("Failed to handle request");
        assert!(resp_axum.contains("Successfully executed ToServer"));

        let req_err = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {
                "name": "from_openapi",
                "arguments": {
                    "subcommand": "invalid_sub"
                }
            }
        })
        .to_string();
        let resp_err = handle_request(&req_err).expect("Failed to handle request");
        assert!(resp_err.contains(r#""isError":true"#));
    }

    #[test]
    fn test_mcp_tools_call_to_openapi() {
        let dir = tempfile::tempdir().expect("Failed to create temporary directory");
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("Failed to create directory");
        std::fs::write(src_dir.join("lib.rs"), "pub struct Empty {}")
            .expect("Failed to write to file");
        let out_file = dir.path().join("openapi.json");

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {
                "name": "to_openapi",
                "arguments": {
                    "input": dir.path().to_str().expect("Failed to convert path to str"),
                    "output": out_file.to_str().expect("Failed to convert path to str")
                }
            }
        })
        .to_string();

        let resp = handle_request(&req).expect("Failed to handle request");
        assert!(resp.contains("Successfully generated OpenAPI spec"));

        let req_missing = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "tools/call",
            "params": {
                "name": "to_openapi",
                "arguments": {}
            }
        })
        .to_string();
        let resp_missing = handle_request(&req_missing).expect("Failed to handle request");
        assert!(resp_missing.contains("Missing required argument: 'input'"));
    }

    #[test]
    fn test_mcp_tools_call_to_docs_json() {
        let dir = tempfile::tempdir().expect("Failed to create temporary directory");
        let spec_path = dir.path().join("openapi.json");
        std::fs::write(
            &spec_path,
            r#"{"openapi":"3.0.0","info":{"title":"T","version":"1"},"paths":{}}"#,
        )
        .expect("Failed to write to file");
        let out_file = dir.path().join("docs.json");

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {
                "name": "to_docs_json",
                "arguments": {
                    "input": spec_path.to_str().expect("Failed to convert path to str"),
                    "output": out_file.to_str().expect("Failed to convert path to str")
                }
            }
        })
        .to_string();

        let resp = handle_request(&req).expect("Failed to handle request");
        assert!(resp.contains("Successfully generated docs JSON"));

        // Without output (returns JSON string directly)
        let req_inline = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 101,
            "method": "tools/call",
            "params": {
                "name": "to_docs_json",
                "arguments": {
                    "input": spec_path.to_str().expect("Failed to convert path to str")
                }
            }
        })
        .to_string();
        let resp_inline = handle_request(&req_inline).expect("Failed to handle request");
        assert!(resp_inline.contains(r#""isError":false"#));

        let req_missing = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tools/call",
            "params": {
                "name": "to_docs_json",
                "arguments": {}
            }
        })
        .to_string();
        let resp_missing = handle_request(&req_missing).expect("Failed to handle request");
        assert!(resp_missing.contains("Missing required argument: 'input'"));

        let req_bad_out = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 12,
            "method": "tools/call",
            "params": {
                "name": "to_docs_json",
                "arguments": {
                    "input": "/nonexistent_file.json",
                    "output": out_file.to_str().expect("path")
                }
            }
        })
        .to_string();
        let resp_bad_out = handle_request(&req_bad_out).expect("Failed to handle request");
        assert!(resp_bad_out.contains(r#""isError":true"#));
    }

    #[test]
    fn test_mcp_tools_call_sync() {
        let dir = tempfile::tempdir().expect("Failed to create temporary directory");

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 13,
            "method": "tools/call",
            "params": {
                "name": "sync",
                "arguments": {
                    "dir": dir.path().to_str().expect("Failed to convert path to str")
                }
            }
        })
        .to_string();

        let resp = handle_request(&req).expect("Failed to handle request");
        assert!(resp.contains("Successfully synced directory"));

        let req_missing = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 14,
            "method": "tools/call",
            "params": {
                "name": "sync"
            }
        })
        .to_string();
        let resp_missing = handle_request(&req_missing).expect("Failed to handle request");
        assert!(resp_missing.contains("Missing required argument: 'dir'"));

        let req_bad_truth = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 15,
            "method": "tools/call",
            "params": {
                "name": "sync",
                "arguments": {
                    "dir": dir.path().to_str().expect("path"),
                    "truth": "invalid_truth"
                }
            }
        })
        .to_string();
        let resp_bad_truth = handle_request(&req_bad_truth).expect("Failed to handle request");
        assert!(resp_bad_truth.contains(r#""isError":true"#));

        // Test from_arazzo and to_arazzo via dispatch_tool_command
        let arazzo_spec = dir.path().join("flow.arazzo.yaml");
        std::fs::write(
            &arazzo_spec,
            "arazzo: 1.1.0\ninfo:\n  title: Flow\n  version: 1.0.0\nsourceDescriptions:\n  - name: api\n    url: ./openapi.yaml\nworkflows:\n  - workflowId: wf1\n    steps:\n      - stepId: s1\n        operationId: op1\n",
        )
        .expect("write arazzo");
        let arazzo_sdk_out = dir.path().join("arazzo_sdk");
        let res_fa = dispatch_tool_command(
            "from_arazzo",
            &json!({
                "subcommand": "to_sdk",
                "input": arazzo_spec.to_str().expect("str"),
                "output": arazzo_sdk_out.to_str().expect("str"),
                "framework": "axum"
            }),
            None,
        )
        .expect("dispatch from_arazzo");
        assert!(res_fa.contains("Successfully executed from_arazzo"));

        let res_fa_default = dispatch_tool_command(
            "from_arazzo",
            &json!({
                "input": arazzo_spec.to_str().expect("str"),
                "output": arazzo_sdk_out.to_str().expect("str")
            }),
            None,
        )
        .expect("dispatch from_arazzo default");
        assert!(res_fa_default.contains("Successfully executed from_arazzo"));

        let arazzo_roundtrip_out = dir.path().join("roundtrip.arazzo.yaml");
        let res_ta = dispatch_tool_command(
            "to_arazzo",
            &json!({
                "input": arazzo_sdk_out.to_str().expect("str"),
                "output": arazzo_roundtrip_out.to_str().expect("str")
            }),
            None,
        )
        .expect("dispatch to_arazzo");
        assert!(res_ta.contains("Successfully generated Arazzo spec"));
        assert!(dispatch_tool_command("to_arazzo", &json!({}), None).is_err());
    }

    #[test]
    fn test_serve_mcp_inner() {
        let input_data = r#"   
{"jsonrpc": "2.0", "id": 1, "method": "initialize"}
{"jsonrpc": "2.0", "method": "notifications/initialized"}
"#;
        let mut reader = Cursor::new(input_data);
        let mut writer = Vec::new();

        serve_mcp_inner(&mut reader, &mut writer).expect("Failed to serve MCP inner");

        let output = String::from_utf8(writer).expect("Failed to convert UTF-8 bytes to String");
        assert!(output.contains("cdd-rust-mcp"));
        assert_eq!(output.lines().count(), 1);
    }

    struct FailWrite;

    impl io::Write for FailWrite {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("write error"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct FailFlush;

    impl io::Write for FailFlush {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("flush error"))
        }
    }

    #[test]
    fn test_serve_mcp_inner_read_error() {
        let mut stdin = ErrorReader;
        let mut buf = [0u8; 4];
        assert!(io::Read::read(&mut stdin, &mut buf).is_err());
        io::BufRead::consume(&mut stdin, 0);

        let mut stdout = Vec::new();
        serve_mcp_inner(&mut stdin, &mut stdout).expect("must succeed");
        assert!(stdout.is_empty());

        let req = "{\"jsonrpc\": \"2.0\", \"id\": 1, \"method\": \"ping\"}\n";
        let mut reader1 = Cursor::new(req);
        let mut bad_writer = FailWrite;
        assert!(serve_mcp_inner(&mut reader1, &mut bad_writer).is_err());
        assert!(io::Write::flush(&mut bad_writer).is_ok());

        let mut reader2 = Cursor::new(req);
        let mut bad_flusher = FailFlush;
        assert!(serve_mcp_inner(&mut reader2, &mut bad_flusher).is_err());

        assert!(run_mcp_server(&McpArgs::default()).is_ok());
    }
}
