#![deny(missing_docs)]

//! # CDD Web Binary
//!
//! Entry point for the Actix Web server.

use actix_web::{App, HttpServer};
use cdd_web::health_check;
use std::net::TcpListener;

fn build_server(listener: TcpListener) -> std::io::Result<actix_web::dev::Server> {
    Ok(HttpServer::new(|| App::new().service(health_check))
        .listen(listener)?
        .run())
}

fn resolve_bind_addr() -> String {
    std::env::var("CDD_WEB_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_string())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let bind_addr = resolve_bind_addr();
    let listener = TcpListener::bind(bind_addr)?;
    let server = build_server(listener)?;

    if std::env::var("CDD_WEB_ONESHOT").is_ok() {
        server.handle().stop(true).await;
    }

    server.await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_oneshot() {
        std::env::set_var("CDD_WEB_BIND", "127.0.0.1:0");
        std::env::set_var("CDD_WEB_ONESHOT", "1");

        let res = main();

        std::env::remove_var("CDD_WEB_BIND");
        std::env::remove_var("CDD_WEB_ONESHOT");

        assert!(res.is_ok());
    }

    #[actix_web::test]
    async fn test_build_server_start_stop() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = build_server(listener).unwrap();
        let handle = server.handle();
        actix_web::rt::spawn(server);
        handle.stop(true).await;
    }
}
