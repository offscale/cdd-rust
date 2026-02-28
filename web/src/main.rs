#![deny(missing_docs)]
//! # Web App
#![cfg(not(tarpaulin_include))]

use actix_web::{web, App, HttpServer};
use cdd_web::{config, handlers::pet::PetStore, health_check};
use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::Mutex;

fn build_server(listener: TcpListener) -> std::io::Result<actix_web::dev::Server> {
    let pet_store = web::Data::new(PetStore {
        pets: Mutex::new(HashMap::new()),
    });

    Ok(HttpServer::new(move || {
        App::new()
            .app_data(pet_store.clone())
            .service(health_check)
            .configure(config)
    })
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
        let handle = server.handle();
        let server_task = actix_web::rt::spawn(server);
        handle.stop(true).await;
        let _ = server_task.await;
        return Ok(());
    }

    server.await
}
