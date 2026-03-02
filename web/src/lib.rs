#![warn(missing_docs)]
#![deny(missing_docs)]
//! # CDD Web Library
//!
//! Contains route handlers, database logic, and schema definitions.

use actix_web::{get, web, HttpResponse, Responder};

/// Re-export diesel so generated models can access `crate::diesel`.
/// Documented
pub use diesel;

/// Auto-generated database schema.
/// Documented
pub mod schema;

/// Documented
pub mod handlers;
/// Data models generated from schema.
/// Documented
pub mod models;
/// Documented
pub mod security;

/// A simple health check handler.
#[get("/health")]
/// Documented
pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

/// Service configurator.
/// Documented
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
            .route(web::post().to(handlers::user::create_users_with_list_input)),
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
    cfg.service(
        web::resource("/user/createWithArray")
            .route(web::post().to(handlers::user::create_users_with_array_input)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_config() {
        let app = test::init_service(App::new().configure(config).app_data(web::Data::new(
            crate::handlers::pet::PetStore {
                pets: std::sync::Mutex::new(std::collections::HashMap::new()),
            },
        )))
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
    }
}
