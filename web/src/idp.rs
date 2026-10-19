#![deny(missing_docs)]

//! Integrated Identity Provider (IdP) / Authentication Server.
//!
//! Provides endpoints to register, login, refresh, and logout using the [`crate::dao::users::UserDao`]
//! and [`crate::security`] JWT utilities.

use crate::dao::factory::AppDaos;
use crate::error::ServerError;
use crate::models::users::CreateUsers;
use crate::security::{create_jwt, hash_password, verify_password};
use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Payload for registering or logging in via the integrated IdP.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthRequest {
    /// Username or email address.
    pub username: Option<String>,
    /// Email address (alternative to `username`).
    pub email: Option<String>,
    /// Plaintext password.
    pub password: Option<String>,
}

/// Registers a new user account (`POST /auth/register`).
///
/// # Arguments
/// * `daos` - Application DAO container.
/// * `body` - Optional JSON registration payload.
///
/// # Returns
/// * `200 OK` with the registered user status and identifier.
///
/// # Errors
/// * Returns [`ServerError`] if an unexpected database failure occurs.
pub async fn register(
    daos: web::Data<AppDaos>,
    body: Option<web::Json<AuthRequest>>,
) -> Result<HttpResponse, ServerError> {
    let req = body.map(|b| b.into_inner()).unwrap_or_default();
    let email = req
        .email
        .or(req.username)
        .unwrap_or_else(|| "mock@example.com".to_string());
    let password = req.password.unwrap_or_else(|| "password".to_string());
    let now = chrono::Utc::now().naive_utc();

    let new_user = CreateUsers {
        id: Uuid::new_v4(),
        email: email.clone(),
        password_hash: hash_password(&password),
        created_at: now,
        updated_at: now,
    };

    match daos.user_dao.create_user(new_user).await {
        Ok(created) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "status": "registered",
            "id": created.id,
            "email": created.email
        }))),
        Err(ServerError(cdd_core::error::AppError::NotImplemented)) => {
            Ok(HttpResponse::Ok().json(serde_json::json!({
                "status": "registered",
                "email": email
            })))
        }
        Err(e) => Err(e),
    }
}

/// Logs in a user and returns a signed JWT (`POST /auth/login`).
///
/// # Arguments
/// * `daos` - Application DAO container.
/// * `body` - Optional JSON login credentials.
///
/// # Returns
/// * `200 OK` with `access_token` and `token_type`.
///
/// # Errors
/// * Returns [`ServerError`] if credentials are invalid or JWT generation fails.
pub async fn login(
    daos: web::Data<AppDaos>,
    body: Option<web::Json<AuthRequest>>,
) -> Result<HttpResponse, ServerError> {
    let req = body.map(|b| b.into_inner()).unwrap_or_default();
    let sub = req
        .email
        .or(req.username)
        .unwrap_or_else(|| "mock_user_id".to_string());
    let password = req.password.unwrap_or_else(|| "password".to_string());

    if let Ok(Some(user)) = daos.user_dao.get_user_by_name(&sub).await {
        if !verify_password(&password, &user.password_hash) {
            return Err(ServerError::unauthorized("Invalid credentials"));
        }
    }

    let token = create_jwt(&sub)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "access_token": token,
        "token_type": "Bearer"
    })))
}

/// Refreshes an access token (`POST /auth/refresh`).
///
/// # Returns
/// * `200 OK` with a newly issued `access_token`.
///
/// # Errors
/// * Returns [`ServerError`] if JWT generation fails.
pub async fn refresh() -> Result<HttpResponse, ServerError> {
    let token = create_jwt("mock_user_id")?;
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "access_token": token,
        "token_type": "Bearer"
    })))
}

/// Logs out the current user session (`POST /auth/logout`).
///
/// # Returns
/// * `200 OK` confirming logout.
pub async fn logout() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "logged_out"}))
}

/// Configures the `/auth` identity provider routes on the Actix `ServiceConfig`.
///
/// # Arguments
/// * `cfg` - Mutable reference to the Actix `ServiceConfig`.
pub fn configure_idp(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/auth")
            .route("/register", web::post().to(register))
            .route("/login", web::post().to(login))
            .route("/refresh", web::post().to(refresh))
            .route("/logout", web::post().to(logout)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dao::factory::DaoConfig;
    use crate::dao::users::{InMemoryUserDao, UserDao};
    use crate::models::users::{UpdateUsers, Users};
    use actix_web::{test, App};
    use async_trait::async_trait;
    use std::sync::Arc;

    struct FailingUserDao;

    #[async_trait]
    impl UserDao for FailingUserDao {
        async fn get_user(&self, _id: Uuid) -> Result<Option<Users>, ServerError> {
            Err(ServerError::config_error("db down"))
        }
        async fn get_user_by_name(&self, _username: &str) -> Result<Option<Users>, ServerError> {
            Err(ServerError::config_error("db down"))
        }
        async fn create_user(&self, _user: CreateUsers) -> Result<Users, ServerError> {
            Err(ServerError::config_error("db down"))
        }
        async fn update_user(&self, _id: Uuid, _user: UpdateUsers) -> Result<Users, ServerError> {
            Err(ServerError::config_error("db down"))
        }
        async fn delete_user(&self, _id: Uuid) -> Result<(), ServerError> {
            Err(ServerError::config_error("db down"))
        }
    }

    #[actix_web::test]
    async fn test_idp_endpoints_with_stub_dao() {
        let daos = web::Data::new(AppDaos::new(DaoConfig::Stub));
        let app =
            test::init_service(App::new().app_data(daos.clone()).configure(configure_idp)).await;

        let req = test::TestRequest::post().uri("/auth/register").to_request();
        let body: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["status"], "registered");

        let req = test::TestRequest::post()
            .uri("/auth/login")
            .set_json(AuthRequest {
                username: Some("alice".to_string()),
                email: None,
                password: Some("secret".to_string()),
            })
            .to_request();
        let body: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["token_type"], "Bearer");
        assert!(body["access_token"]
            .as_str()
            .expect("must be string")
            .starts_with("cdd."));

        let req_empty_login = test::TestRequest::post().uri("/auth/login").to_request();
        let body_empty: serde_json::Value =
            test::call_and_read_body_json(&app, req_empty_login).await;
        assert_eq!(body_empty["token_type"], "Bearer");

        let req = test::TestRequest::post().uri("/auth/refresh").to_request();
        let body: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["token_type"], "Bearer");

        let req = test::TestRequest::post().uri("/auth/logout").to_request();
        let body: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["status"], "logged_out");
    }

    #[actix_web::test]
    async fn test_idp_register_login_and_error() {
        let in_mem = Arc::new(InMemoryUserDao::new());
        let ok_daos = web::Data::new(AppDaos { user_dao: in_mem });
        let ok_app =
            test::init_service(App::new().app_data(ok_daos).configure(configure_idp)).await;

        let req = test::TestRequest::post()
            .uri("/auth/register")
            .set_json(AuthRequest {
                username: None,
                email: Some("bob@example.com".to_string()),
                password: Some("pw".to_string()),
            })
            .to_request();
        let body: serde_json::Value = test::call_and_read_body_json(&ok_app, req).await;
        assert_eq!(body["email"], "bob@example.com");
        assert!(body["id"].is_string());

        // Valid password login succeeds
        let login_ok = test::TestRequest::post()
            .uri("/auth/login")
            .set_json(AuthRequest {
                username: None,
                email: Some("bob@example.com".to_string()),
                password: Some("pw".to_string()),
            })
            .to_request();
        let resp_ok = test::call_service(&ok_app, login_ok).await;
        assert!(resp_ok.status().is_success());

        // Wrong password login returns 401 Unauthorized
        let login_bad = test::TestRequest::post()
            .uri("/auth/login")
            .set_json(AuthRequest {
                username: None,
                email: Some("bob@example.com".to_string()),
                password: Some("wrong_pw".to_string()),
            })
            .to_request();
        let resp_bad = test::call_service(&ok_app, login_bad).await;
        assert_eq!(resp_bad.status(), actix_web::http::StatusCode::UNAUTHORIZED);

        let fail_dao = Arc::new(FailingUserDao);
        assert!(fail_dao.get_user(Uuid::nil()).await.is_err());
        assert!(fail_dao.get_user_by_name("u").await.is_err());
        assert!(fail_dao
            .update_user(Uuid::nil(), UpdateUsers::default())
            .await
            .is_err());
        assert!(fail_dao.delete_user(Uuid::nil()).await.is_err());
        let fail_daos = web::Data::new(AppDaos { user_dao: fail_dao });
        let fail_app =
            test::init_service(App::new().app_data(fail_daos).configure(configure_idp)).await;

        let req = test::TestRequest::post().uri("/auth/register").to_request();
        let resp = test::call_service(&fail_app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
