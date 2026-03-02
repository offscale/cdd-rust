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

#[derive(Deserialize, Debug)]
struct RpcRequest {
    jsonrpc: String,
    method: String,
    params: Option<serde_json::Value>,
    id: Option<serde_json::Value>,
}

#[derive(Serialize, Debug)]
struct RpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
    id: Option<serde_json::Value>,
}

#[derive(Serialize, Debug)]
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
