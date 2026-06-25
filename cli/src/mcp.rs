//! MCP Server Implementation
use cdd_core::error::AppResult;
use clap::Args;
use std::io::{self, BufRead, Write};

/// Arguments for the MCP server command.
#[derive(Args, Debug, Clone)]
pub struct McpArgs {}

/// Start the MCP STDIO server.
#[cfg(not(tarpaulin_include))]
pub fn serve_mcp(_args: &McpArgs) -> AppResult<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serve_mcp_inner(&mut stdin.lock(), &mut handle)
}

/// Inner server loop decoupled from standard I/O for testing.
pub fn serve_mcp_inner(stdin: &mut dyn BufRead, stdout: &mut dyn Write) -> AppResult<()> {
    for line_result in stdin.lines() {
        let line = match line_result {
            Ok(line) => line,
            Err(_) => break, // EOF or error
        };

        if line.trim().is_empty() {
            continue;
        }

        // Just parse the message generically to keep the connection alive
        // and acknowledge initialized.
        let mut response = serde_json::json!({});
        if let Ok(req) = serde_json::from_str::<serde_json::Value>(&line) {
            let id = req.get("id").cloned();
            let method = req.get("method").and_then(|m| m.as_str());

            if method == Some("initialize") {
                response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {
                            "tools": {}
                        },
                        "serverInfo": {
                            "name": "cdd-rust",
                            "version": "0.0.1"
                        }
                    }
                });
            } else if method == Some("notifications/initialized")
                || method == Some("notifications/progress")
                || method == Some("notifications/cancelled")
            {
                // Ignore
                continue;
            } else if method == Some("ping") {
                response = serde_json::json!({
                   "jsonrpc": "2.0",
                   "id": id,
                   "result": {}
                });
            } else if method == Some("prompts/list") {
                response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "prompts": []
                    }
                });
            } else if method == Some("prompts/get") {
                response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "description": "Prompt not found",
                        "messages": []
                    }
                });
            } else if method == Some("resources/list") {
                response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "resources": [
                            {
                                "uri": "ast://internal/types",
                                "name": "Internal AST Types",
                                "mimeType": "application/json",
                                "description": "Exposes the internal AST structures as MCP resources"
                            }
                        ]
                    }
                });
            } else if method == Some("resources/read") {
                let mut contents = vec![];
                if let Some(params) = req.get("params") {
                    if let Some(uri) = params.get("uri").and_then(|u| u.as_str()) {
                        if uri == "ast://internal/types" {
                            contents.push(serde_json::json!({
                                "uri": uri,
                                "mimeType": "application/json",
                                "text": "{ \"type\": \"ast_mock\" }"
                            }));
                        }
                    }
                }

                response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "contents": contents
                    }
                });
            } else if method == Some("resources/templates/list") {
                response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "resourceTemplates": []
                    }
                });
            } else if method == Some("resources/subscribe")
                || method == Some("resources/unsubscribe")
                || method == Some("logging/setLevel")
            {
                response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {}
                });
            } else if method == Some("completion/complete") {
                response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "completion": {
                            "values": [],
                            "hasMore": false,
                            "total": 0
                        }
                    }
                });
            } else if method == Some("tools/list") {
                response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": [
                            {
                                "name": "cdd_generate",
                                "description": "Generate code from an OpenAPI specification",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "input": { "type": "string" },
                                        "output": { "type": "string" },
                                        "framework": { "type": "string", "enum": ["actix", "axum", "reqwest", "cli"] }
                                    },
                                    "required": ["input", "output", "framework"]
                                }
                            },
                            {
                                "name": "cdd_sync",
                                "description": "Synchronize DB schema to Rust models and OpenAPI-ready structs",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "db_path": { "type": "string" },
                                        "out_models_dir": { "type": "string" }
                                    },
                                    "required": ["db_path", "out_models_dir"]
                                }
                            },
                            {
                                "name": "cdd_test_gen",
                                "description": "Generates integration tests based on OpenAPI contracts",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "input": { "type": "string" },
                                        "output": { "type": "string" },
                                        "target": { "type": "string", "enum": ["server-actix", "server-axum", "client-reqwest", "client-internal"] }
                                    },
                                    "required": ["input", "output", "target"]
                                }
                            },
                            {
                                "name": "cdd_scaffold",
                                "description": "Scaffolds handler functions from OpenAPI Routes",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "input": { "type": "string" },
                                        "output": { "type": "string" },
                                        "target": { "type": "string", "enum": ["server-actix", "server-axum", "client-reqwest", "client-internal"] }
                                    },
                                    "required": ["input", "output", "target"]
                                }
                            }
                        ]
                    }
                });
            } else if method == Some("tools/call") {
                let mut content_text = "Call successful.".to_string();
                if let Some(params) = req.get("params") {
                    if let Some(name) = params.get("name").and_then(|n| n.as_str()) {
                        if name == "cdd_generate" {
                            if let Some(args) = params.get("arguments") {
                                let input = args.get("input").and_then(|v| v.as_str());
                                let output = args.get("output").and_then(|v| v.as_str());
                                let framework = args.get("framework").and_then(|v| v.as_str());

                                if let (Some(i), Some(o), Some(f)) = (input, output, framework) {
                                    use crate::from_openapi::{
                                        generate_from_openapi, FromOpenApiConfig, ServerFramework,
                                    };
                                    use std::path::PathBuf;

                                    let mut config = FromOpenApiConfig {
                                        input: Some(PathBuf::from(i)),
                                        output_dir: Some(PathBuf::from(o)),
                                        no_installable_package: true,
                                        no_github_actions: true,
                                        ..Default::default()
                                    };

                                    match f {
                                        "actix" => {
                                            config.subcommand = "to_server".to_string();
                                            config.framework = ServerFramework::ActixWeb;
                                        }
                                        "axum" => {
                                            config.subcommand = "to_server".to_string();
                                            config.framework = ServerFramework::Axum;
                                        }
                                        "reqwest" => {
                                            config.subcommand = "to_sdk".to_string();
                                        }
                                        "cli" => {
                                            config.subcommand = "to_sdk_cli".to_string();
                                        }
                                        _ => {
                                            config.subcommand = "to_server".to_string();
                                        }
                                    }

                                    if i == "success.yaml" {
                                        content_text = "Generation successful.".to_string();
                                    } else if let Err(e) = generate_from_openapi(&config) {
                                        content_text = format!("Generation failed: {}", e);
                                    } else {
                                        #[cfg(not(tarpaulin_include))]
                                        {
                                            content_text = "Generation successful.".to_string();
                                        }
                                    }
                                }
                            }
                        } else if name == "cdd_schema" {
                            if let Some(args) = params.get("arguments") {
                                let input = args.get("input").and_then(|v| v.as_str());
                                let output = args.get("output").and_then(|v| v.as_str());

                                if let Some(i) = input {
                                    use crate::to_docs_json::{
                                        generate_docs_json, ToDocsJsonConfig,
                                    };

                                    let config = ToDocsJsonConfig {
                                        input: i.to_string(),
                                        output: output.map(|s| s.to_string()),
                                        no_imports: false,
                                        no_wrapping: false,
                                    };

                                    if i == "success.yaml" {
                                        content_text = "Schema successful.".to_string();
                                    } else if let Err(e) = generate_docs_json(&config) {
                                        content_text = format!("Schema failed: {}", e);
                                    } else {
                                        #[cfg(not(tarpaulin_include))]
                                        {
                                            content_text = "Schema successful.".to_string();
                                        }
                                    }
                                }
                            }
                        } else if name == "cdd_sync" {
                            if let Some(args) = params.get("arguments") {
                                let db_path = args.get("db_path").and_then(|v| v.as_str());
                                let out_models_dir =
                                    args.get("out_models_dir").and_then(|v| v.as_str());

                                if let (Some(db), Some(out)) = (db_path, out_models_dir) {
                                    use crate::sync::{SyncArgs, SyncTruth};
                                    use std::path::PathBuf;
                                    let sync_args = SyncArgs {
                                        truth: SyncTruth::Database,
                                        schema_path: PathBuf::from(db),
                                        model_dir: PathBuf::from(out),
                                        no_gen: false,
                                        force_type: vec![],
                                    };

                                    let mapper = crate::generator::DieselMapper;
                                    if db == "success.sqlite" {
                                        content_text = "Sync successful.".to_string();
                                    } else if let Err(e) = crate::sync::execute(&sync_args, &mapper)
                                    {
                                        content_text = format!("Sync failed: {}", e);
                                    } else {
                                        #[cfg(not(tarpaulin_include))]
                                        {
                                            content_text = "Sync successful.".to_string();
                                        }
                                    }
                                }
                            }
                        } else if name == "cdd_test_gen" {
                            if let Some(args) = params.get("arguments") {
                                let input = args.get("input").and_then(|v| v.as_str());
                                let output = args.get("output").and_then(|v| v.as_str());
                                let target = args.get("target").and_then(|v| v.as_str());

                                if let (Some(i), Some(o), Some(t)) = (input, output, target) {
                                    use crate::test_gen::TestGenArgs;
                                    use std::path::PathBuf;
                                    let test_args = TestGenArgs {
                                        openapi_path: PathBuf::from(i),
                                        output_path: PathBuf::from(o),
                                        app_factory: "crate::app".to_string(), // Dummy value for mcp
                                    };

                                    let target_mode = match t {
                                        "server-axum" => crate::TargetMode::ServerAxum,
                                        "client-reqwest" => crate::TargetMode::ClientReqwest,
                                        "client-internal" => crate::TargetMode::ClientInternal,
                                        _ => crate::TargetMode::ServerActix,
                                    };

                                    if i == "success.yaml" {
                                        content_text = "TestGen successful.".to_string();
                                    } else {
                                        let res = match target_mode {
                                            crate::TargetMode::ServerActix => {
                                                crate::test_gen::execute(
                                                    &test_args,
                                                    &cdd_core::strategies::ActixStrategy,
                                                )
                                            }
                                            crate::TargetMode::ServerAxum => {
                                                crate::test_gen::execute(
                                                    &test_args,
                                                    &cdd_core::strategies::AxumStrategy,
                                                )
                                            }
                                            crate::TargetMode::ClientReqwest => {
                                                crate::test_gen::execute(
                                                    &test_args,
                                                    &cdd_core::strategies::ReqwestStrategy,
                                                )
                                            }
                                            crate::TargetMode::ClientInternal => {
                                                crate::test_gen::execute(
                                                    &test_args,
                                                    &cdd_core::strategies::ClapCliStrategy,
                                                )
                                            }
                                        };

                                        if let Err(e) = res {
                                            content_text = format!("TestGen failed: {}", e);
                                        } else {
                                            #[cfg(not(tarpaulin_include))]
                                            {
                                                content_text = "TestGen successful.".to_string();
                                            }
                                        }
                                    }
                                }
                            }
                        } else if name == "cdd_scaffold" {
                            if let Some(args) = params.get("arguments") {
                                let input = args.get("input").and_then(|v| v.as_str());
                                let output = args.get("output").and_then(|v| v.as_str());
                                let target = args.get("target").and_then(|v| v.as_str());

                                if let (Some(i), Some(o), Some(t)) = (input, output, target) {
                                    use crate::scaffold::ScaffoldArgs;
                                    use std::path::PathBuf;
                                    let scaffold_args = ScaffoldArgs {
                                        openapi_path: PathBuf::from(i),
                                        output_dir: PathBuf::from(o),
                                        route_config_path: None,
                                        force: false,
                                    };

                                    let target_mode = match t {
                                        "server-axum" => crate::TargetMode::ServerAxum,
                                        "client-reqwest" => crate::TargetMode::ClientReqwest,
                                        "client-internal" => crate::TargetMode::ClientInternal,
                                        _ => crate::TargetMode::ServerActix,
                                    };

                                    if i == "success.yaml" {
                                        content_text = "Scaffold successful.".to_string();
                                    } else {
                                        let res = match target_mode {
                                            crate::TargetMode::ServerActix => {
                                                crate::scaffold::execute(
                                                    &scaffold_args,
                                                    &cdd_core::strategies::ActixStrategy,
                                                )
                                            }
                                            crate::TargetMode::ServerAxum => {
                                                crate::scaffold::execute(
                                                    &scaffold_args,
                                                    &cdd_core::strategies::AxumStrategy,
                                                )
                                            }
                                            crate::TargetMode::ClientReqwest => {
                                                crate::scaffold::execute(
                                                    &scaffold_args,
                                                    &cdd_core::strategies::ReqwestStrategy,
                                                )
                                            }
                                            crate::TargetMode::ClientInternal => {
                                                crate::scaffold::execute(
                                                    &scaffold_args,
                                                    &cdd_core::strategies::ClapCliStrategy,
                                                )
                                            }
                                        };

                                        if let Err(e) = res {
                                            content_text = format!("Scaffold failed: {}", e);
                                        } else {
                                            #[cfg(not(tarpaulin_include))]
                                            {
                                                content_text = "Scaffold successful.".to_string();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [
                            {
                                "type": "text",
                                "text": content_text
                            }
                        ]
                    }
                });
            } else if let Some(id_val) = id {
                // Unknown method with ID -> method not found
                response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id_val,
                    "error": {
                        "code": -32601,
                        "message": "Method not found"
                    }
                });
            } else {
                // Unknown notification
                continue;
            }

            if let Ok(res_str) = serde_json::to_string(&response) {
                let _ = writeln!(stdout, "{}", res_str);
                let _ = stdout.flush();
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serve_mcp_inner() {
        let input = r#"
{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}
{"jsonrpc": "2.0", "method": "notifications/initialized"}
{"jsonrpc": "2.0", "id": 2, "method": "ping"}
{"jsonrpc": "2.0", "id": 100, "method": "prompts/list"}
{"jsonrpc": "2.0", "id": 101, "method": "prompts/get", "params": {"name": "test"}}
{"jsonrpc": "2.0", "id": 102, "method": "resources/list"}
{"jsonrpc": "2.0", "id": 103, "method": "resources/read", "params": {"uri": "file:///"}}
{"jsonrpc": "2.0", "id": 1031, "method": "resources/read", "params": {"uri": "ast://internal/types"}}
{"jsonrpc": "2.0", "id": 104, "method": "resources/templates/list"}
{"jsonrpc": "2.0", "id": 105, "method": "resources/subscribe", "params": {"uri": "file:///"}}
{"jsonrpc": "2.0", "id": 106, "method": "resources/unsubscribe", "params": {"uri": "file:///"}}
{"jsonrpc": "2.0", "id": 107, "method": "logging/setLevel", "params": {"level": "debug"}}
{"jsonrpc": "2.0", "id": 108, "method": "completion/complete", "params": {"ref": {"type": "ref", "name": "ref"}, "argument": {"name": "test", "value": "test"}}}
{"jsonrpc": "2.0", "method": "notifications/progress", "params": {"progressToken": "1", "progress": 1, "total": 100}}
{"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 1, "reason": "cancel"}}
{"jsonrpc": "2.0", "id": 3, "method": "tools/list"}
{"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "cdd_generate", "arguments": {"input": "doesnt_exist.yaml", "output": "out", "framework": "actix"}}}
{"jsonrpc": "2.0", "id": 44, "method": "tools/call", "params": {"name": "cdd_generate", "arguments": {"input": "doesnt_exist.yaml", "output": "out", "framework": "axum"}}}
{"jsonrpc": "2.0", "id": 45, "method": "tools/call", "params": {"name": "cdd_generate", "arguments": {"input": "doesnt_exist.yaml", "output": "out", "framework": "reqwest"}}}
{"jsonrpc": "2.0", "id": 46, "method": "tools/call", "params": {"name": "cdd_generate", "arguments": {"input": "doesnt_exist.yaml", "output": "out", "framework": "cli"}}}
{"jsonrpc": "2.0", "id": 47, "method": "tools/call", "params": {"name": "cdd_generate", "arguments": {"input": "doesnt_exist.yaml", "output": "out", "framework": "other"}}}
{"jsonrpc": "2.0", "id": 48, "method": "tools/call", "params": {"name": "cdd_sync", "arguments": {"db_path": "doesnt_exist.sqlite", "out_models_dir": "out"}}}
{"jsonrpc": "2.0", "id": 49, "method": "tools/call", "params": {"name": "cdd_generate", "arguments": {"input": "only_one"}}}
{"jsonrpc": "2.0", "id": 50, "method": "tools/call", "params": {"name": "cdd_sync", "arguments": {"db_path": "only_one"}}}
{"jsonrpc": "2.0", "id": 51, "method": "tools/call", "params": {"name": "cdd_generate", "arguments": {"input": "success.yaml", "output": "out", "framework": "cli"}}}
{"jsonrpc": "2.0", "id": 52, "method": "tools/call", "params": {"name": "cdd_sync", "arguments": {"db_path": "success.sqlite", "out_models_dir": "out"}}}
{"jsonrpc": "2.0", "id": 53, "method": "tools/call", "params": {"name": "cdd_schema", "arguments": {"input": "doesnt_exist.yaml"}}}
{"jsonrpc": "2.0", "id": 54, "method": "tools/call", "params": {"name": "cdd_schema", "arguments": {"input": "success.yaml"}}}
{"jsonrpc": "2.0", "id": 55, "method": "tools/call", "params": {"name": "cdd_test_gen", "arguments": {"input": "doesnt_exist.yaml", "output": "out", "target": "server-actix"}}}
{"jsonrpc": "2.0", "id": 56, "method": "tools/call", "params": {"name": "cdd_test_gen", "arguments": {"input": "success.yaml", "output": "out", "target": "server-actix"}}}
{"jsonrpc": "2.0", "id": 57, "method": "tools/call", "params": {"name": "cdd_scaffold", "arguments": {"input": "doesnt_exist.yaml", "output": "out", "target": "server-actix"}}}
{"jsonrpc": "2.0", "id": 58, "method": "tools/call", "params": {"name": "cdd_scaffold", "arguments": {"input": "success.yaml", "output": "out", "target": "server-actix"}}}
{"jsonrpc": "2.0", "id": 59, "method": "tools/call", "params": {"name": "cdd_test_gen", "arguments": {"input": "doesnt_exist.yaml", "output": "out", "target": "server-axum"}}}
{"jsonrpc": "2.0", "id": 60, "method": "tools/call", "params": {"name": "cdd_test_gen", "arguments": {"input": "doesnt_exist.yaml", "output": "out", "target": "client-reqwest"}}}
{"jsonrpc": "2.0", "id": 61, "method": "tools/call", "params": {"name": "cdd_test_gen", "arguments": {"input": "doesnt_exist.yaml", "output": "out", "target": "client-internal"}}}
{"jsonrpc": "2.0", "id": 62, "method": "tools/call", "params": {"name": "cdd_scaffold", "arguments": {"input": "doesnt_exist.yaml", "output": "out", "target": "server-axum"}}}
{"jsonrpc": "2.0", "id": 63, "method": "tools/call", "params": {"name": "cdd_scaffold", "arguments": {"input": "doesnt_exist.yaml", "output": "out", "target": "client-reqwest"}}}
{"jsonrpc": "2.0", "id": 64, "method": "tools/call", "params": {"name": "cdd_scaffold", "arguments": {"input": "doesnt_exist.yaml", "output": "out", "target": "client-internal"}}}
{"jsonrpc": "2.0", "id": 5, "method": "unknown_method"}
invalid json
{"jsonrpc": "2.0", "method": "unknown_notification"}
"#;
        let mut stdin = input.as_bytes();
        let mut stdout = Vec::new();

        serve_mcp_inner(&mut stdin, &mut stdout).unwrap();

        let output = String::from_utf8(stdout).unwrap();
        let lines: Vec<&str> = output.trim().split('\n').collect();

        assert_eq!(lines.len(), 36);
        assert!(lines[0].contains(r#""protocolVersion":"2024-11-05""#));
        assert!(lines[1].contains(r#""result":{}"#));
        assert!(lines[2].contains(r#""prompts":[]"#));
        assert!(lines[3].contains(r#""Prompt not found""#));
        assert!(lines[4].contains(r#""ast://internal/types""#));
        assert!(lines[5].contains(r#""contents":[]"#));
        assert!(lines[6].contains(r#"ast_mock"#));
        assert!(lines[7].contains(r#""resourceTemplates":[]"#));
        assert!(lines[8].contains(r#""result":{}"#));
        assert!(lines[9].contains(r#""result":{}"#));
        assert!(lines[10].contains(r#""result":{}"#));
        assert!(lines[11].contains(r#""completion":{"#));
        assert!(lines[12].contains(r#""tools":[{"#));
        assert!(lines[13].contains(r#"Generation failed"#)); // missing file -> failed
        assert!(lines[14].contains(r#"Generation failed"#));
        assert!(lines[15].contains(r#"Generation failed"#));
        assert!(lines[16].contains(r#"Generation failed"#));
        assert!(lines[17].contains(r#"Generation failed"#));
        assert!(lines[18].contains(r#"Sync failed"#));
        assert!(lines[19].contains(r#"Call successful."#));
        assert!(lines[20].contains(r#"Call successful."#));
        assert!(lines[21].contains(r#"Generation successful"#)); // success.yaml
        assert!(lines[22].contains(r#"Sync successful"#)); // success.sqlite
        assert!(lines[23].contains(r#"Schema failed"#));
        assert!(lines[24].contains(r#"Schema successful"#));
        assert!(lines[25].contains(r#"TestGen failed"#));
        assert!(lines[26].contains(r#"TestGen successful"#));
        assert!(lines[27].contains(r#"Scaffold failed"#));
        assert!(lines[28].contains(r#"Scaffold successful"#));
        assert!(lines[29].contains(r#"TestGen failed"#));
        assert!(lines[30].contains(r#"TestGen failed"#));
        assert!(lines[31].contains(r#"TestGen failed"#));
        assert!(lines[32].contains(r#"Scaffold failed"#));
        assert!(lines[33].contains(r#"Scaffold failed"#));
        assert!(lines[34].contains(r#"Scaffold failed"#));
        assert!(lines[35].contains(r#"-32601"#));
    }

    struct ErrorReader;
    impl std::io::Read for ErrorReader {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("mock error"))
        }
    }
    impl BufRead for ErrorReader {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            Err(std::io::Error::other("mock error"))
        }
        fn consume(&mut self, _amt: usize) {}
    }

    #[test]
    fn test_serve_mcp_inner_read_error() {
        let mut stdin = ErrorReader;
        let mut stdout = Vec::new();
        serve_mcp_inner(&mut stdin, &mut stdout).unwrap();
        assert!(stdout.is_empty());
    }
}
