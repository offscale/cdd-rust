#![warn(missing_docs)]
#![deny(missing_docs)]

//! # CDD Web Library
//!
//! Contains HTTP route handlers, database access objects, authentication, validation,
//! and schema definitions for the CDD reference web server.
#![cfg(not(target_os = "wasi"))]

use actix_web::{get, web, HttpResponse, Responder};

/// Re-export `diesel` so generated models can reference `crate::diesel`.
pub use diesel;

/// Auto-generated database schema definitions.
pub mod schema;

/// Data Access Objects (DAOs) and connection pool management.
pub mod dao;
/// Centralized server error enum and HTTP response error mapping.
pub mod error;
/// HTTP route handlers for Pet, Store, and User endpoints.
pub mod handlers;
/// Integrated Identity Provider (IdP) authentication endpoints.
pub mod idp;
/// Domain and database models.
pub mod models;
/// Authentication and authorization middleware and security extractors.
pub mod security;
/// Fake data generator and database seeder.
pub mod seeder;
/// Request validation middleware.
pub mod validation;
/// Administrative webhook dispatch endpoints.
pub mod webhooks;

/// Health check endpoint (`GET /health`).
///
/// # Returns
/// * `200 OK` with `"OK"` body when the service is healthy.
#[get("/health")]
pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

/// Configures all Pet, Store, and User routes on the provided Actix `ServiceConfig`.
///
/// # Arguments
/// * `cfg` - Mutable reference to the Actix Web service configuration.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/pet").route(web::post().to(handlers::pet::add_pet)));
    cfg.service(web::resource("/pet").route(web::put().to(handlers::pet::update_pet)));
    cfg.service(
        web::resource("/pet/findByStatus").route(web::get().to(handlers::pet::find_pets_by_status)),
    );
    cfg.service(
        web::resource("/pet/findByTags").route(web::get().to(handlers::pet::find_pets_by_tags)),
    );
    cfg.service(web::resource("/pet/{petId}").route(web::get().to(handlers::pet::get_pet_by_id)));
    cfg.service(
        web::resource("/pet/{petId}").route(web::post().to(handlers::pet::update_pet_with_form)),
    );
    cfg.service(web::resource("/pet/{petId}").route(web::delete().to(handlers::pet::delete_pet)));
    cfg.service(
        web::resource("/pet/{petId}/uploadImage").route(web::post().to(handlers::pet::upload_file)),
    );
    cfg.service(
        web::resource("/store/inventory").route(web::get().to(handlers::store::get_inventory)),
    );
    cfg.service(web::resource("/store/order").route(web::post().to(handlers::store::place_order)));
    cfg.service(
        web::resource("/store/order/{orderId}")
            .route(web::get().to(handlers::store::get_order_by_id)),
    );
    cfg.service(
        web::resource("/store/order/{orderId}")
            .route(web::delete().to(handlers::store::delete_order)),
    );
    cfg.service(web::resource("/user").route(web::post().to(handlers::user::create_user)));
    cfg.service(
        web::resource("/user/createWithArray")
            .route(web::post().to(handlers::user::create_users_with_array_input)),
    );
    cfg.service(
        web::resource("/user/createWithList")
            .route(web::post().to(handlers::user::create_users_with_list_input)),
    );
    cfg.service(web::resource("/user/login").route(web::get().to(handlers::user::login_user)));
    cfg.service(web::resource("/user/logout").route(web::get().to(handlers::user::logout_user)));
    cfg.service(
        web::resource("/user/{username}").route(web::get().to(handlers::user::get_user_by_name)),
    );
    cfg.service(
        web::resource("/user/{username}").route(web::put().to(handlers::user::update_user)),
    );
    cfg.service(
        web::resource("/user/{username}").route(web::delete().to(handlers::user::delete_user)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_config() {
        let app = test::init_service(
            App::new()
                .configure(config)
                .app_data(web::Data::new(crate::handlers::pet::PetStore::default()))
                .app_data(web::Data::new(crate::handlers::store::OrderStore::default()))
                .app_data(web::Data::new(crate::dao::factory::AppDaos::new(
                    crate::dao::factory::DaoConfig::Stub,
                ))),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/store/inventory")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_health_check() {
        let app = test::init_service(App::new().service(health_check)).await;
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body = test::read_body(resp).await;
        assert_eq!(body.as_ref(), b"OK");
    }
}
