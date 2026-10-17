#![deny(missing_docs)]
#![cfg(feature = "server")]

//! JSON-RPC Server Module
//!
//! Exposes `cdd-cli` commands over an HTTP JSON-RPC 2.0 endpoint by delegating
//! to the unified `crate::mcp::dispatch_tool_command` registry.

use crate::from_openapi::FromOpenApiSubcommand;
use crate::mcp::dispatch_tool_command;
use actix_web::{web, HttpResponse, Responder};
#[cfg(not(coverage))]
use actix_web::{App, HttpServer};
use cdd_core::error::AppResult;
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Arguments for the `serve_json_rpc` command.
#[derive(Args, Debug, Clone)]
pub struct ServeJsonRpcArgs {
    /// Port to listen on.
    #[clap(short = 'p', long, default_value_t = 8080, env = "CDD_PORT")]
    pub port: u16,

    /// Host address to bind to.
    #[clap(short = 'l', long, default_value = "127.0.0.1", env = "CDD_LISTEN")]
    pub listen: String,
}

/// JSON-RPC 2.0 Request Envelope.
#[derive(Deserialize, Debug)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    params: Option<Value>,
    id: Option<Value>,
}

/// JSON-RPC 2.0 Response Envelope.
#[derive(Serialize, Debug)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    id: Value,
}

/// JSON-RPC 2.0 Error Object.
#[derive(Serialize, Debug)]
struct JsonRpcError {
    code: i32,
    message: String,
}

async fn handle_rpc(body: String) -> impl Responder {
    let req: JsonRpcRequest = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            return HttpResponse::Ok().json(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(JsonRpcError {
                    code: -32700,
                    message: format!("Parse error: {}", e),
                }),
                id: Value::Null,
            });
        }
    };

    let id = req.id.unwrap_or(Value::Null);

    if req.jsonrpc != "2.0" {
        return HttpResponse::Ok().json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: -32600,
                message: "Invalid Request: jsonrpc must be '2.0'".to_string(),
            }),
            id,
        });
    }

    let params = req.params.unwrap_or_else(|| json!({}));

    let response = match req.method.as_str() {
        "version" => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(json!({ "version": env!("CARGO_PKG_VERSION") })),
            error: None,
            id,
        },
        "to_openapi" => match dispatch_tool_command("to_openapi", &params, None) {
            Ok(_) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(json!({"status": "success", "message": "Generated OpenAPI spec"})),
                error: None,
                id,
            },
            Err(e) => {
                let code = if e.to_string().contains("Missing required argument") {
                    -32602
                } else {
                    -32000
                };
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(JsonRpcError {
                        code,
                        message: e.to_string(),
                    }),
                    id,
                }
            }
        },
        "to_docs_json" => match dispatch_tool_command("to_docs_json", &params, None) {
            Ok(out_str) => {
                let result_val = if params.get("output").is_some() {
                    json!({"status": "success", "message": "Generated docs JSON"})
                } else {
                    serde_json::from_str(&out_str).unwrap_or(Value::String(out_str))
                };
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(result_val),
                    error: None,
                    id,
                }
            }
            Err(e) => {
                let code = if e.to_string().contains("Missing required argument") {
                    -32602
                } else {
                    -32000
                };
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(JsonRpcError {
                        code,
                        message: e.to_string(),
                    }),
                    id,
                }
            }
        },
        "from_openapi" | "to_sdk" | "to_sdk_cli" | "to_server" => {
            match dispatch_tool_command(
                req.method.as_str(),
                &params,
                Some(FromOpenApiSubcommand::ToSdk),
            ) {
                Ok(_) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(json!({"status": "success", "message": "Executed from_openapi"})),
                    error: None,
                    id,
                },
                Err(e) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32000,
                        message: e.to_string(),
                    }),
                    id,
                },
            }
        }
        "sync" => match dispatch_tool_command("sync", &params, None) {
            Ok(_) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(json!({"status": "success", "message": "Synced project"})),
                error: None,
                id,
            },
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(JsonRpcError {
                    code: -32000,
                    message: e.to_string(),
                }),
                id,
            },
        },
        _ => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", req.method),
            }),
            id,
        },
    };

    HttpResponse::Ok().json(response)
}

/// Runs the JSON-RPC server over HTTP.
///
/// # Arguments
/// * `args` - Server bind host and port configuration.
///
/// # Returns
/// `Ok(())` when the server stops cleanly.
///
/// # Errors
/// Returns [`cdd_core::error::AppError`] if binding or running the HTTP server fails.
pub fn run_serve_json_rpc(args: &ServeJsonRpcArgs) -> AppResult<()> {
    let config = ServeJsonRpcConfig {
        port: args.port,
        listen: args.listen.clone(),
    };
    serve_json_rpc(&config)
}

/// Configuration for `serve_json_rpc` programmatic API.
#[derive(Debug, Default, Clone)]
pub struct ServeJsonRpcConfig {
    /// Port to listen on.
    pub port: u16,
    /// Host address to bind to.
    pub listen: String,
}

#[allow(dead_code)]
fn configure_rpc_app(cfg: &mut web::ServiceConfig) {
    cfg.route("/", web::post().to(handle_rpc));
}

/// Start the JSON-RPC server.
///
/// # Arguments
/// * `config` - Programmatic configuration for `serve_json_rpc`.
///
/// # Returns
/// `Ok(())` when the server stops cleanly.
///
/// # Errors
/// Returns [`cdd_core::error::AppError`] if binding or running the HTTP server fails.
pub fn serve_json_rpc(config: &ServeJsonRpcConfig) -> AppResult<()> {
    let bind_addr = format!("{}:{}", config.listen, config.port);
    println!("Starting JSON-RPC server on http://{}", bind_addr);

    let listener = std::net::TcpListener::bind(&bind_addr)?;
    #[cfg(not(coverage))]
    if std::env::var_os("CDD_MCP_TEST_EOF").is_none() {
        let sys = actix_web::rt::System::new();
        sys.block_on(async move {
            HttpServer::new(|| App::new().configure(configure_rpc_app))
                .listen(listener)?
                .run()
                .await
        })?;
    }
    #[cfg(coverage)]
    let _ = listener;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use tempfile::tempdir;

    #[actix_rt::test]
    async fn test_rpc_parse_error_and_invalid_version() {
        let app = test::init_service(App::new().configure(configure_rpc_app)).await;
        let req = test::TestRequest::post()
            .uri("/")
            .set_payload("invalid json")
            .to_request();
        let resp: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp["error"]["code"], -32700);

        let req_ver = test::TestRequest::post()
            .uri("/")
            .set_payload(r#"{"jsonrpc": "1.0", "method": "version", "id": 1}"#)
            .to_request();
        let resp_ver: Value = test::call_and_read_body_json(&app, req_ver).await;
        assert_eq!(resp_ver["error"]["code"], -32600);
    }

    #[actix_rt::test]
    async fn test_rpc_version() {
        let app = test::init_service(App::new().route("/", web::post().to(handle_rpc))).await;
        let req = test::TestRequest::post()
            .uri("/")
            .set_payload(r#"{"jsonrpc": "2.0", "method": "version", "id": 1}"#)
            .to_request();
        let resp: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp["result"]["version"], env!("CARGO_PKG_VERSION"));
    }

    #[actix_rt::test]
    async fn test_rpc_unknown_method() {
        let app = test::init_service(App::new().route("/", web::post().to(handle_rpc))).await;
        let req = test::TestRequest::post()
            .uri("/")
            .set_payload(r#"{"jsonrpc": "2.0", "method": "unknown", "id": 1}"#)
            .to_request();
        let resp: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[actix_rt::test]
    async fn test_rpc_to_openapi() {
        let dir = tempdir().expect("Failed to create temporary directory");
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("Failed to create directory");
        std::fs::write(src_dir.join("lib.rs"), "pub struct Empty {}")
            .expect("Failed to write to file");
        let out_file = dir.path().join("openapi.json");

        let app = test::init_service(App::new().route("/", web::post().to(handle_rpc))).await;

        let payload = json!({
            "jsonrpc": "2.0",
            "method": "to_openapi",
            "params": {
                "input": dir.path().to_str().expect("Failed to convert path to str"),
                "output": out_file.to_str().expect("Failed to convert path to str")
            },
            "id": 1
        })
        .to_string();
        let req = test::TestRequest::post()
            .uri("/")
            .set_payload(payload)
            .to_request();
        let resp: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp["result"]["status"], "success");

        let req_missing = test::TestRequest::post()
            .uri("/")
            .set_payload(r#"{"jsonrpc": "2.0", "method": "to_openapi", "params": {}, "id": 2}"#)
            .to_request();
        let resp_missing: Value = test::call_and_read_body_json(&app, req_missing).await;
        assert_eq!(resp_missing["error"]["code"], -32602);

        let req_err = test::TestRequest::post()
            .uri("/")
            .set_payload(r#"{"jsonrpc": "2.0", "method": "to_openapi", "params": {"input": "/does/not/exist"}, "id": 3}"#)
            .to_request();
        let resp_err: Value = test::call_and_read_body_json(&app, req_err).await;
        assert_eq!(resp_err["error"]["code"], -32000);
    }

    #[actix_rt::test]
    async fn test_rpc_to_docs_json() {
        let dir = tempdir().expect("Failed to create temporary directory");
        let spec_path = dir.path().join("openapi.json");
        std::fs::write(
            &spec_path,
            r#"{"openapi":"3.0.0","info":{"title":"T","version":"1"},"paths":{}}"#,
        )
        .expect("Failed to write to file");
        let out_file = dir.path().join("docs.json");

        let app = test::init_service(App::new().route("/", web::post().to(handle_rpc))).await;

        let payload_out = json!({
            "jsonrpc": "2.0",
            "method": "to_docs_json",
            "params": {
                "input": spec_path.to_str().expect("Failed to convert path to str"),
                "output": out_file.to_str().expect("Failed to convert path to str")
            },
            "id": 1
        })
        .to_string();
        let req_out = test::TestRequest::post()
            .uri("/")
            .set_payload(payload_out)
            .to_request();
        let resp_out: Value = test::call_and_read_body_json(&app, req_out).await;
        assert_eq!(resp_out["result"]["status"], "success");

        let payload_no_out = json!({
            "jsonrpc": "2.0",
            "method": "to_docs_json",
            "params": {
                "input": spec_path.to_str().expect("Failed to convert path to str")
            },
            "id": 2
        })
        .to_string();
        let req_no_out = test::TestRequest::post()
            .uri("/")
            .set_payload(payload_no_out)
            .to_request();
        let resp_no_out: Value = test::call_and_read_body_json(&app, req_no_out).await;
        assert!(resp_no_out["result"].is_array());

        let req_missing = test::TestRequest::post()
            .uri("/")
            .set_payload(r#"{"jsonrpc": "2.0", "method": "to_docs_json", "params": {}, "id": 3}"#)
            .to_request();
        let resp_missing: Value = test::call_and_read_body_json(&app, req_missing).await;
        assert_eq!(resp_missing["error"]["code"], -32602);

        let req_bad_file = test::TestRequest::post()
            .uri("/")
            .set_payload(r#"{"jsonrpc": "2.0", "method": "to_docs_json", "params": {"input": "/does/not/exist.yaml"}, "id": 4}"#)
            .to_request();
        let resp_bad_file: Value = test::call_and_read_body_json(&app, req_bad_file).await;
        assert_eq!(resp_bad_file["error"]["code"], -32000);
    }

    #[actix_rt::test]
    async fn test_rpc_from_openapi_and_sync() {
        let dir = tempdir().expect("Failed to create temporary directory");
        let spec_path = dir.path().join("openapi.json");
        std::fs::write(
            &spec_path,
            r#"{"openapi":"3.0.0","info":{"title":"T","version":"1"},"paths":{}}"#,
        )
        .expect("Failed to write to file");
        let out_dir = dir.path().join("out");

        let app = test::init_service(App::new().route("/", web::post().to(handle_rpc))).await;

        let payload = json!({
            "jsonrpc": "2.0",
            "method": "to_sdk",
            "params": {
                "input": spec_path.to_str().expect("Failed to convert path to str"),
                "output_dir": out_dir.to_str().expect("Failed to convert path to str"),
                "framework": "axum"
            },
            "id": 1
        })
        .to_string();
        let req = test::TestRequest::post()
            .uri("/")
            .set_payload(payload)
            .to_request();
        let resp: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp["result"]["status"], "success");

        let req_err = test::TestRequest::post()
            .uri("/")
            .set_payload(r#"{"jsonrpc": "2.0", "method": "from_openapi", "params": {"subcommand": "bad"}, "id": 2}"#)
            .to_request();
        let resp_err: Value = test::call_and_read_body_json(&app, req_err).await;
        assert_eq!(resp_err["error"]["code"], -32000);

        let sync_payload = json!({
            "jsonrpc": "2.0",
            "method": "sync",
            "params": {
                "dir": dir.path().to_str().expect("Failed to convert path to str")
            },
            "id": 3
        })
        .to_string();
        let req_sync = test::TestRequest::post()
            .uri("/")
            .set_payload(sync_payload)
            .to_request();
        let resp_sync: Value = test::call_and_read_body_json(&app, req_sync).await;
        assert_eq!(resp_sync["result"]["status"], "success");

        let req_sync_err = test::TestRequest::post()
            .uri("/")
            .set_payload(r#"{"jsonrpc": "2.0", "method": "sync", "params": {}, "id": 4}"#)
            .to_request();
        let resp_sync_err: Value = test::call_and_read_body_json(&app, req_sync_err).await;
        assert_eq!(resp_sync_err["error"]["code"], -32000);
    }

    #[core::prelude::v1::test]
    fn test_serve_json_rpc_bind_error() {
        let args = ServeJsonRpcArgs {
            port: 80,
            listen: "invalid.host.address.local".to_string(),
        };
        assert!(run_serve_json_rpc(&args).is_err());

        std::env::set_var("CDD_MCP_TEST_EOF", "1");
        let ok_args = ServeJsonRpcArgs {
            port: 0,
            listen: "127.0.0.1".to_string(),
        };
        assert!(run_serve_json_rpc(&ok_args).is_ok());
    }
}
