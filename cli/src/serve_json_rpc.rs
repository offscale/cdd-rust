#![deny(missing_docs)]
#![cfg(feature = "server")]

//! JSON-RPC Server Module
//!
//! Exposes `cdd-cli` commands over an HTTP JSON-RPC 2.0 endpoint by delegating
//! to the unified `crate::mcp::dispatch_tool_command` registry.

use crate::from_openapi::FromOpenApiSubcommand;
use crate::mcp::dispatch_tool_command;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use cdd_core::error::{AppError, AppResult};
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
    /// JSON-RPC protocol version (`"2.0"`).
    jsonrpc: String,
    /// Target RPC method name.
    method: String,
    /// Optional method parameters payload.
    params: Option<Value>,
    /// Optional request correlation identifier.
    id: Option<Value>,
}

/// JSON-RPC 2.0 Response Envelope.
#[derive(Serialize, Debug)]
struct JsonRpcResponse {
    /// JSON-RPC protocol version (`"2.0"`).
    jsonrpc: String,
    /// Successful result payload when no error occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    /// Error object when the call failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    /// Request correlation identifier.
    id: Value,
}

/// JSON-RPC 2.0 Error Object.
#[derive(Serialize, Debug)]
struct JsonRpcError {
    /// Numeric JSON-RPC error code.
    code: i32,
    /// Human-readable error message.
    message: String,
}

/// Handles incoming HTTP JSON-RPC 2.0 requests and dispatches them to CLI tools.
///
/// # Arguments
/// * `body` - Raw HTTP request body string.
/// * `shutdown_notify` - Optional notification handle used to trigger graceful server shutdown in tests.
///
/// # Returns
/// An [`actix_web::Responder`] serializing the JSON-RPC 2.0 response envelope.
async fn handle_rpc(
    body: String,
    shutdown_notify: Option<web::Data<std::sync::Arc<tokio::sync::Notify>>>,
) -> impl Responder {
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
        "shutdown" => {
            shutdown_notify
                .iter()
                .for_each(|notify| notify.notify_one());
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(json!({ "status": "shutting_down" })),
                error: None,
                id,
            }
        }
        "to_openapi" => match dispatch_tool_command("to_openapi", &params, None) {
            Ok(_) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(json!({"status": "success", "message": "Generated OpenAPI spec"})),
                error: None,
                id,
            },
            Err(e) => {
                let code = if matches!(e, AppError::MissingArgument { .. }) {
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
                let code = if matches!(e, AppError::MissingArgument { .. }) {
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
        "from_arazzo" | "to_arazzo" => {
            match dispatch_tool_command(req.method.as_str(), &params, None) {
                Ok(msg) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: Some(json!({"status": "success", "message": msg})),
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

/// Configures the Actix Web service routes for the JSON-RPC 2.0 endpoint.
///
/// # Arguments
/// * `cfg` - Mutable Actix [`web::ServiceConfig`] to register routes on.
pub fn configure_rpc_app(cfg: &mut web::ServiceConfig) {
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
    let sys = actix_web::rt::System::new();
    let run_res = sys.block_on(async move {
        let notify = std::sync::Arc::new(tokio::sync::Notify::new());
        let notify_data = web::Data::new(notify.clone());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(notify_data.clone())
                .configure(configure_rpc_app)
        })
        .workers(1)
        .listen(listener)?
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(async move {
            notify.notified().await;
            std::mem::drop(handle.stop(false));
        });
        server.await
    });
    run_res.map_err(Into::into)
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

        let arazzo_spec = dir.path().join("rpc.arazzo.yaml");
        std::fs::write(
            &arazzo_spec,
            "arazzo: 1.1.0\ninfo:\n  title: RpcFlow\n  version: 1.0.0\nsourceDescriptions:\n  - name: api\n    url: ./openapi.yaml\nworkflows:\n  - workflowId: wf1\n    steps:\n      - stepId: s1\n        operationId: op1\n",
        )
        .expect("write arazzo");
        let arazzo_out = dir.path().join("rpc_arazzo_out");
        let fa_payload = json!({
            "jsonrpc": "2.0",
            "method": "from_arazzo",
            "params": {
                "input": arazzo_spec.to_str().expect("str"),
                "output": arazzo_out.to_str().expect("str")
            },
            "id": 5
        })
        .to_string();
        let req_fa = test::TestRequest::post()
            .uri("/")
            .set_payload(fa_payload)
            .to_request();
        let resp_fa: Value = test::call_and_read_body_json(&app, req_fa).await;
        assert_eq!(resp_fa["result"]["status"], "success");

        let req_ta_err = test::TestRequest::post()
            .uri("/")
            .set_payload(r#"{"jsonrpc": "2.0", "method": "to_arazzo", "params": {}, "id": 6}"#)
            .to_request();
        let resp_ta_err: Value = test::call_and_read_body_json(&app, req_ta_err).await;
        assert_eq!(resp_ta_err["error"]["code"], -32000);
    }

    #[core::prelude::v1::test]
    fn test_serve_json_rpc_bind_error() {
        let args = ServeJsonRpcArgs {
            port: 80,
            listen: "invalid.host.address.local".to_string(),
        };
        assert!(run_serve_json_rpc(&args).is_err());

        let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("ephemeral port");
        let port = probe.local_addr().expect("local addr").port();
        drop(probe);

        let client_thread = std::thread::spawn(move || {
            use std::io::{Read, Write};
            std::thread::sleep(std::time::Duration::from_millis(50));
            let body = r#"{"jsonrpc":"2.0","method":"shutdown","id":1}"#;
            let req = format!(
                "POST / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect");
            stream.write_all(req.as_bytes()).expect("write request");
            let mut resp = String::new();
            let _ = stream.read_to_string(&mut resp);
            assert!(resp.contains("shutting_down"));
        });

        let ok_args = ServeJsonRpcArgs {
            port,
            listen: "127.0.0.1".to_string(),
        };
        assert!(run_serve_json_rpc(&ok_args).is_ok());
        client_thread.join().expect("client thread");
    }
}
