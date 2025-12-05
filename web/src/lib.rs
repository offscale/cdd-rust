#![deny(missing_docs)]

//! # CDD Web Library
//!
//! Contains route handlers, database logic, and schema definitions.

use actix_web::{get, HttpResponse, Responder};

/// Re-export diesel so generated models can access `crate::diesel`.
pub use diesel;

/// Auto-generated database schema.
pub mod schema;

/// Data models generated from schema.
pub mod models;

/// A simple health check handler.
#[get("/health")]
pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_health_check() {
        let app = test::init_service(App::new().service(health_check)).await;
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}
