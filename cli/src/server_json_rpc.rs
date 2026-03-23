#![cfg(feature = "server")]
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use cdd_core::error::AppResult;
use clap::Args;
use serde::{Deserialize, Serialize};

#[derive(Args, Debug, Clone)]
pub struct ServerJsonRpcArgs {
    #[clap(short, long, default_value = "8080", env = "CDD_RPC_PORT")]
    pub port: u16,

    #[clap(short, long, default_value = "127.0.0.1", env = "CDD_RPC_LISTEN")]
    pub listen: String,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
struct RpcRequest {
    jsonrpc: String,
    method: String,
    #[allow(dead_code)]
    params: Option<serde_json::Value>,
    id: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct RpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
    id: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct RpcError {
    code: i32,
    message: String,
}

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

#[cfg(not(tarpaulin_include))]
#[cfg(not(tarpaulin_include))]
pub fn execute(args: &ServerJsonRpcArgs) -> AppResult<()> {
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
        assert_eq!(resp.error.unwrap().code, -32600);
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
        assert_eq!(resp.error.unwrap().code, -32601);
    }
}
