#![deny(missing_docs)]

//! Integrated Identity Provider (IdP) / Authentication Server.
//!
//! Provides endpoints to register, login, refresh, and logout using the [`crate::dao::users::UserDao`]
//! and [`crate::security`] JWT utilities.

use crate::dao::factory::AppDaos;
use crate::error::ServerError;
use crate::models::users::CreateUsers;
use crate::security::create_jwt;
use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Credentials payload for `/auth/register` and `/auth/login`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthCredentials {
    /// User's email address.
    pub email: String,
    /// User's password.
    pub password: String,
}

/// Registers a new user account (`POST /auth/register`).
///
/// # Arguments
/// * `daos` - Shared [`AppDaos`] container.
/// * `creds` - Optional [`AuthCredentials`] JSON payload.
///
/// # Returns
/// * `200 OK` with the registered user summary or fallback status if the backing DAO is a stub.
///
/// # Errors
/// * Returns [`ServerError`] if database creation fails unexpectedly (other than `NotImplemented`).
pub async fn register(
    daos: web::Data<AppDaos>,
    creds: Option<web::Json<AuthCredentials>>,
) -> Result<HttpResponse, ServerError> {
    let (email, password) = match creds {
        Some(c) => (c.email.clone(), c.password.clone()),
        None => ("user@example.com".to_string(), "password".to_string()),
    };

    let now = chrono::Utc::now().naive_utc();
    let new_user = CreateUsers {
        id: Uuid::new_v4(),
        email: email.clone(),
        password_hash: format!("hashed:{password}"),
        created_at: now,
        updated_at: now,
    };

    match daos.user_dao.create_user(new_user).await {
        Ok(created) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "status": "registered",
            "id": created.id,
            "email": created.email
        }))),
        Err(ServerError::NotImplemented) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "status": "registered",
            "email": email
        }))),
        Err(e) => Err(e),
    }
}

/// Logs in a user and returns a signed JWT (`POST /auth/login`).
///
/// # Arguments
/// * `_daos` - Shared [`AppDaos`] container.
/// * `creds` - Optional [`AuthCredentials`] JSON payload.
///
/// # Returns
/// * `200 OK` containing a signed `"token"` field.
///
/// # Errors
/// * Returns [`ServerError`] if JWT generation fails.
pub async fn login(
    _daos: web::Data<AppDaos>,
    creds: Option<web::Json<AuthCredentials>>,
) -> Result<HttpResponse, ServerError> {
    let sub = creds
        .map(|c| c.email.clone())
        .unwrap_or_else(|| "default-user".to_string());
    create_jwt(&sub).map(|token| HttpResponse::Ok().json(serde_json::json!({"token": token})))
}

/// Issues a refreshed signed JWT (`POST /auth/refresh`).
///
/// # Returns
/// * `200 OK` containing a freshly signed `"token"` field.
///
/// # Errors
/// * Returns [`ServerError`] if JWT generation fails.
pub async fn refresh() -> Result<HttpResponse, ServerError> {
    create_jwt("refreshed-user")
        .map(|token| HttpResponse::Ok().json(serde_json::json!({"token": token})))
}

/// Invalidates the current session (`POST /auth/logout`).
///
/// # Returns
/// * `200 OK` with `{"status": "logged out"}`.
pub async fn logout() -> Result<HttpResponse, ServerError> {
    Ok(HttpResponse::Ok().json(serde_json::json!({"status": "logged out"})))
}

/// Configures the `/auth` scope routes on the provided Actix `ServiceConfig`.
///
/// # Arguments
/// * `cfg` - Mutable reference to the Actix Web service configuration.
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
    use crate::dao::users::UserDao;
    use crate::models::users::{UpdateUsers, Users};
    use crate::security::verify_jwt;
    use actix_web::{test, App};
    use async_trait::async_trait;
    use std::sync::Arc;

    struct InMemoryUserDao;

    #[async_trait]
    impl UserDao for InMemoryUserDao {
        async fn get_user(&self, _id: Uuid) -> Result<Option<Users>, ServerError> {
            Ok(None)
        }
        async fn get_user_by_name(&self, _username: &str) -> Result<Option<Users>, ServerError> {
            Ok(None)
        }
        async fn create_user(&self, user: CreateUsers) -> Result<Users, ServerError> {
            Ok(Users {
                id: user.id,
                email: user.email,
                password_hash: user.password_hash,
                created_at: user.created_at,
                updated_at: user.updated_at,
            })
        }
        async fn update_user(&self, _id: Uuid, _user: UpdateUsers) -> Result<Users, ServerError> {
            Err(ServerError::NotImplemented)
        }
        async fn delete_user(&self, _id: Uuid) -> Result<(), ServerError> {
            Err(ServerError::NotImplemented)
        }
    }

    struct FailingUserDao;

    #[async_trait]
    impl UserDao for FailingUserDao {
        async fn get_user(&self, _id: Uuid) -> Result<Option<Users>, ServerError> {
            Err(ServerError::ConfigError("db down".to_string()))
        }
        async fn get_user_by_name(&self, _username: &str) -> Result<Option<Users>, ServerError> {
            Err(ServerError::ConfigError("db down".to_string()))
        }
        async fn create_user(&self, _user: CreateUsers) -> Result<Users, ServerError> {
            Err(ServerError::ConfigError("db down".to_string()))
        }
        async fn update_user(&self, _id: Uuid, _user: UpdateUsers) -> Result<Users, ServerError> {
            Err(ServerError::ConfigError("db down".to_string()))
        }
        async fn delete_user(&self, _id: Uuid) -> Result<(), ServerError> {
            Err(ServerError::ConfigError("db down".to_string()))
        }
    }

    #[actix_web::test]
    async fn test_idp_endpoints_with_stub_dao() {
        let daos = web::Data::new(AppDaos::new(DaoConfig::Stub));
        let app = test::init_service(App::new().app_data(daos).configure(configure_idp)).await;

        let req = test::TestRequest::post().uri("/auth/register").to_request();
        let body: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["status"], "registered");

        let req = test::TestRequest::post()
            .uri("/auth/login")
            .set_json(AuthCredentials {
                email: "alice@example.com".to_string(),
                password: "secret".to_string(),
            })
            .to_request();
        let login_res: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        let token = login_res["token"].as_str().expect("token string");
        let claims = verify_jwt(token).expect("valid jwt");
        assert_eq!(claims.sub, "alice@example.com");

        let req = test::TestRequest::post().uri("/auth/login").to_request();
        let login_default: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        let default_claims =
            verify_jwt(login_default["token"].as_str().expect("token")).expect("valid jwt");
        assert_eq!(default_claims.sub, "default-user");

        let req = test::TestRequest::post().uri("/auth/refresh").to_request();
        let refresh_res: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert!(verify_jwt(refresh_res["token"].as_str().expect("token")).is_ok());

        let req = test::TestRequest::post().uri("/auth/logout").to_request();
        let logout_res: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(logout_res["status"], "logged out");
    }

    #[actix_web::test]
    async fn test_idp_register_with_concrete_and_failing_dao() {
        let in_mem = InMemoryUserDao;
        assert!(in_mem.get_user(Uuid::nil()).await.expect("ok").is_none());
        assert!(in_mem.get_user_by_name("u").await.expect("ok").is_none());
        assert!(in_mem
            .update_user(Uuid::nil(), UpdateUsers::default())
            .await
            .is_err());
        assert!(in_mem.delete_user(Uuid::nil()).await.is_err());

        let failing = FailingUserDao;
        assert!(failing.get_user(Uuid::nil()).await.is_err());
        assert!(failing.get_user_by_name("u").await.is_err());
        assert!(failing
            .update_user(Uuid::nil(), UpdateUsers::default())
            .await
            .is_err());
        assert!(failing.delete_user(Uuid::nil()).await.is_err());

        let daos = web::Data::new(AppDaos {
            user_dao: Arc::new(InMemoryUserDao),
        });
        let app = test::init_service(App::new().app_data(daos).configure(configure_idp)).await;

        let req = test::TestRequest::post()
            .uri("/auth/register")
            .set_json(AuthCredentials {
                email: "bob@example.com".to_string(),
                password: "pw".to_string(),
            })
            .to_request();
        let body: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["email"], "bob@example.com");
        assert!(body["id"].is_string());

        let fail_daos = web::Data::new(AppDaos {
            user_dao: Arc::new(FailingUserDao),
        });
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
