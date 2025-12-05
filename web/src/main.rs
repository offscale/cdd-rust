#![deny(missing_docs)]

//! # CDD Web Binary
//!
//! Entry point for the Actix Web server.

use actix_web::{App, HttpServer};
use cdd_web::health_check;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(health_check))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
