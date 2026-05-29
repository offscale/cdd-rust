//! JSON-RPC Server Module
#![cfg(feature = "server")]
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use cdd_core::error::AppResult;
use clap::Args;
use serde::{Deserialize, Serialize};

use crate::from_openapi::{generate_from_openapi, FromOpenApiConfig, ServerFramework};
use crate::to_docs_json::{generate_docs_json, ToDocsJsonConfig};
use crate::to_openapi::{generate_to_openapi, ToOpenApiConfig};
use std::path::PathBuf;

/// Arguments for the JSON-RPC server command.
#[derive(Args, Debug, Clone)]
pub struct ServeJsonRpcArgs {
    /// Port to listen on.
    #[clap(short, long, default_value = "8080", env = "CDD_PORT")]
    pub port: u16,

    /// Interface to listen on.
    #[clap(short, long, default_value = "127.0.0.1", env = "CDD_LISTEN")]
    pub listen: String,
}

/// JSON-RPC Request schema.
#[derive(Deserialize, Serialize, Debug, PartialEq)]
struct RpcRequest {
    /// JSON-RPC version
    jsonrpc: String,
    /// Method name
    method: String,
    /// Parameters
    #[allow(dead_code)]
    params: Option<serde_json::Value>,
    /// Request ID
    id: Option<serde_json::Value>,
}

/// JSON-RPC Response schema.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct RpcResponse {
    /// JSON-RPC version
    jsonrpc: String,
    /// Result payload
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    /// Error payload
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
    /// Request ID
    id: Option<serde_json::Value>,
}

/// JSON-RPC Error schema.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct RpcError {
    /// Error code
    code: i32,
    /// Error message
    message: String,
}

/// Handler for the JSON-RPC POST endpoint.
async fn handle_rpc(req: web::Json<RpcRequest>) -> impl Responder {
    if req.jsonrpc != "2.0" {
        return HttpResponse::Ok().json(RpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(RpcError {
                code: -32600,
                message: "Invalid Request".to_string(),
            }),
            id: req.id.clone(),
        });
    }

    match req.method.as_str() {
        "version" => HttpResponse::Ok().json(RpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(serde_json::json!("0.0.1")),
            error: None,
            id: req.id.clone(),
        }),
        "to_openapi" => {
            if let Some(serde_json::Value::Object(map)) = &req.params {
                if let (
                    Some(serde_json::Value::String(input)),
                    Some(serde_json::Value::String(output)),
                ) = (map.get("input"), map.get("output"))
                {
                    let config = ToOpenApiConfig {
                        input: PathBuf::from(input),
                        output: PathBuf::from(output),
                    };
                    match generate_to_openapi(&config) {
                        Ok(_) => HttpResponse::Ok().json(RpcResponse {
                            jsonrpc: "2.0".to_string(),
                            result: Some(serde_json::json!("Success")),
                            error: None,
                            id: req.id.clone(),
                        }),
                        Err(e) => HttpResponse::Ok().json(RpcResponse {
                            jsonrpc: "2.0".to_string(),
                            result: None,
                            error: Some(RpcError {
                                code: -32000,
                                message: format!("{}", e),
                            }),
                            id: req.id.clone(),
                        }),
                    }
                } else {
                    HttpResponse::Ok().json(RpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: None,
                        error: Some(RpcError {
                            code: -32602,
                            message: "Invalid params for to_openapi".to_string(),
                        }),
                        id: req.id.clone(),
                    })
                }
            } else {
                HttpResponse::Ok().json(RpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(RpcError {
                        code: -32602,
                        message: "Invalid params".to_string(),
                    }),
                    id: req.id.clone(),
                })
            }
        }
        "to_docs_json" => {
            if let Some(serde_json::Value::Object(map)) = &req.params {
                if let Some(serde_json::Value::String(input)) = map.get("input") {
                    let output = map
                        .get("output")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let no_imports = map
                        .get("no_imports")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let no_wrapping = map
                        .get("no_wrapping")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let config = ToDocsJsonConfig {
                        input: input.to_string(),
                        output,
                        no_imports,
                        no_wrapping,
                    };
                    match generate_docs_json(&config) {
                        Ok(_) => HttpResponse::Ok().json(RpcResponse {
                            jsonrpc: "2.0".to_string(),
                            result: Some(serde_json::json!("Success")),
                            error: None,
                            id: req.id.clone(),
                        }),
                        Err(e) => HttpResponse::Ok().json(RpcResponse {
                            jsonrpc: "2.0".to_string(),
                            result: None,
                            error: Some(RpcError {
                                code: -32000,
                                message: format!("{}", e),
                            }),
                            id: req.id.clone(),
                        }),
                    }
                } else {
                    HttpResponse::Ok().json(RpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: None,
                        error: Some(RpcError {
                            code: -32602,
                            message: "Invalid params for to_docs_json".to_string(),
                        }),
                        id: req.id.clone(),
                    })
                }
            } else {
                HttpResponse::Ok().json(RpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(RpcError {
                        code: -32602,
                        message: "Invalid params".to_string(),
                    }),
                    id: req.id.clone(),
                })
            }
        }
        "from_openapi_to_sdk" | "from_openapi_to_sdk_cli" | "from_openapi_to_server" => {
            if let Some(serde_json::Value::Object(map)) = &req.params {
                let subcommand = match req.method.as_str() {
                    "from_openapi_to_sdk" => "to_sdk".to_string(),
                    "from_openapi_to_sdk_cli" => "to_sdk_cli".to_string(),
                    "from_openapi_to_server" => "to_server".to_string(),
                    _ => "".to_string(),
                };

                let input = map.get("input").and_then(|v| v.as_str()).map(PathBuf::from);
                let input_dir = map
                    .get("input_dir")
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from);
                let output_dir = map
                    .get("output")
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from);
                let no_github_actions = map
                    .get("no_github_actions")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let no_installable_package = map
                    .get("no_installable_package")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let tests = map.get("tests").and_then(|v| v.as_bool()).unwrap_or(false);

                let config = FromOpenApiConfig {
                    subcommand,
                    input,
                    input_dir,
                    output_dir,
                    no_github_actions,
                    no_installable_package,
                    tests,
                    framework: ServerFramework::ActixWeb, // default
                };

                match generate_from_openapi(&config) {
                    Ok(_) => HttpResponse::Ok().json(RpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: Some(serde_json::json!("Success")),
                        error: None,
                        id: req.id.clone(),
                    }),
                    Err(e) => HttpResponse::Ok().json(RpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: None,
                        error: Some(RpcError {
                            code: -32000,
                            message: format!("{}", e),
                        }),
                        id: req.id.clone(),
                    }),
                }
            } else {
                HttpResponse::Ok().json(RpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(RpcError {
                        code: -32602,
                        message: "Invalid params".to_string(),
                    }),
                    id: req.id.clone(),
                })
            }
        }
        _ => HttpResponse::Ok().json(RpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(RpcError {
                code: -32601,
                message: "Method not found".to_string(),
            }),
            id: req.id.clone(),
        }),
    }
}

/// Starts the JSON-RPC server based on provided arguments.
#[cfg(not(tarpaulin_include))]
#[cfg(not(tarpaulin_include))]
pub fn execute(args: &ServeJsonRpcArgs) -> AppResult<()> {
    let listen = args.listen.clone();
    let port = args.port;

    actix_rt::System::new()
        .block_on(async move {
            println!("Starting JSON-RPC server on {}:{}", listen, port);
            let server = HttpServer::new(|| App::new().route("/", web::post().to(handle_rpc)))
                .bind((listen, port))?;
            server.run().await
        })
        .map_err(|e| cdd_core::error::AppError::General(format!("Server error: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test;

    #[actix_rt::test]
    async fn test_rpc_version() {
        let req = RpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "version".to_string(),
            #[allow(dead_code)]
            params: None,
            id: Some(serde_json::json!(1)),
        };

        let app = test::init_service(
            actix_web::App::new().route("/", actix_web::web::post().to(handle_rpc)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/")
            .set_json(&req)
            .to_request();

        let resp: RpcResponse = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.result, Some(serde_json::json!("0.0.1")));
    }

    #[actix_rt::test]
    async fn test_rpc_invalid_jsonrpc() {
        let req = RpcRequest {
            jsonrpc: "1.0".to_string(),
            method: "version".to_string(),
            #[allow(dead_code)]
            params: None,
            id: Some(serde_json::json!(1)),
        };

        let app = test::init_service(
            actix_web::App::new().route("/", actix_web::web::post().to(handle_rpc)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/")
            .set_json(&req)
            .to_request();

        let resp: RpcResponse = test::call_and_read_body_json(&app, req).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.expect("Missing error").code, -32600);
    }

    #[actix_rt::test]
    async fn test_rpc_method_not_found() {
        let req = RpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "unknown_method".to_string(),
            #[allow(dead_code)]
            params: None,
            id: Some(serde_json::json!(1)),
        };

        let app = test::init_service(
            actix_web::App::new().route("/", actix_web::web::post().to(handle_rpc)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/")
            .set_json(&req)
            .to_request();

        let resp: RpcResponse = test::call_and_read_body_json(&app, req).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.expect("Missing error").code, -32601);
    }
}

/// Configuration for `serve_json_rpc` programmatic API
#[derive(Debug)]
pub struct ServeJsonRpcConfig {
    pub port: u16,
    pub listen: String,
}

impl Default for ServeJsonRpcConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            listen: "127.0.0.1".to_string(),
        }
    }
}

/// Expose CLI interface as a JSON-RPC server.
#[cfg(all(feature = "server", not(target_os = "wasi")))]
pub fn serve_json_rpc(config: &ServeJsonRpcConfig) -> AppResult<()> {
    let args = ServeJsonRpcArgs {
        port: config.port,
        listen: config.listen.clone(),
    };
    execute(&args)
}

#[cfg(any(not(feature = "server"), target_os = "wasi"))]
pub fn serve_json_rpc(config: &ServeJsonRpcConfig) -> AppResult<()> {
    Err(cdd_core::error::AppError::General(
        "serve_json_rpc is not supported on this target.".to_string(),
    ))
}
