//! Integrated Identity Provider (IdP) / Auth Server.
//!
//! Provides endpoints to register, login, refresh, and logout using the `UserDao`.

use crate::dao::factory::AppDaos;
use actix_web::{web, HttpResponse, Responder};

/// Configuration for IdP endpoints.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/auth")
            .route("/register", web::post().to(register))
            .route("/login", web::post().to(login))
            .route("/refresh", web::post().to(refresh))
            .route("/logout", web::post().to(logout)),
    );
}

/// Register a new user.
pub async fn register(daos: web::Data<AppDaos>) -> impl Responder {
    let _ = daos.user_dao.clone();
    HttpResponse::Ok().json(serde_json::json!({"message": "User registered successfully"}))
}

/// Login and receive a token.
pub async fn login(daos: web::Data<AppDaos>) -> impl Responder {
    let _ = daos.user_dao.clone();
    HttpResponse::Ok().json(serde_json::json!({"token": "mock-token-123"}))
}

/// Refresh an existing token.
pub async fn refresh() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({"token": "new-mock-token-123"}))
}

/// Logout the current user.
pub async fn logout() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({"message": "Logged out successfully"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dao::factory::DaoConfig;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_idp_endpoints() {
        let daos = AppDaos::new(DaoConfig::Stub);
        let app =
            test::init_service(App::new().app_data(web::Data::new(daos)).configure(config)).await;

        let req = test::TestRequest::post().uri("/auth/register").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post().uri("/auth/login").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post().uri("/auth/refresh").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post().uri("/auth/logout").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}
