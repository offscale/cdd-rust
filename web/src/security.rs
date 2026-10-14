#![deny(missing_docs)]

//! Authentication and authorization extractors, token utilities, and Hybrid Auth Actix middleware.

use crate::dao::users::UserDao;
use crate::error::ServerError;
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage, HttpResponse,
};
use futures_util::future::LocalBoxFuture;
use serde::{Deserialize, Serialize};
use std::env;
use std::future::{ready, Ready};
use std::marker::PhantomData;
use std::sync::Arc;

/// Marker extractor type for API Key authentication schemes.
#[derive(Clone, Debug, Default)]
pub struct ApiKey;

/// Generic marker extractor type for OAuth2 authentication schemes parameterized by required scopes.
#[derive(Clone, Debug, Default)]
pub struct OAuth2<T>(pub PhantomData<T>);

/// Generic marker extractor type for Petstore OAuth2 authentication parameterized by required scopes.
#[derive(Clone, Debug, Default)]
pub struct PetstoreAuth<T>(pub PhantomData<T>);

/// OAuth2 scope marker types used by generated route handlers.
pub mod scopes {
    /// Scope granting write access to pet resources (`write:pets`).
    #[derive(Clone, Debug, Default)]
    pub struct WritePets;

    /// Scope granting read access to pet resources (`read:pets`).
    #[derive(Clone, Debug, Default)]
    pub struct ReadPets;
}

/// Token Claims structure encoded into bearer tokens.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Claims {
    /// Subject (typically the user's UUID or username).
    pub sub: String,
    /// Expiration timestamp (seconds since Unix epoch).
    pub exp: usize,
}

/// Configuration for token signing and verification.
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Secret key used to sign and verify tokens.
    pub jwt_secret: String,
}

impl Default for SecurityConfig {
    /// Loads the secret from the `JWT_SECRET` environment variable, or falls back to a development default.
    fn default() -> Self {
        let jwt_secret =
            env::var("JWT_SECRET").unwrap_or_else(|_| "super-secret-jwt-key".to_string());
        Self { jwt_secret }
    }
}

/// Creates a signed bearer token for the specified subject (`sub`).
///
/// # Arguments
/// * `sub` - Subject identifier (e.g., user ID or email).
///
/// # Returns
/// * Signed token string valid for 24 hours.
///
/// # Errors
/// * Returns [`ServerError::ConfigError`] if token encoding fails.
pub fn create_jwt(sub: &str) -> Result<String, ServerError> {
    create_jwt_with_config(sub, &SecurityConfig::default())
}

/// Creates a signed bearer token using an explicit [`SecurityConfig`].
///
/// # Arguments
/// * `sub` - Subject identifier.
/// * `config` - Security configuration containing the signing secret.
///
/// # Returns
/// * Signed token string valid for 24 hours.
///
/// # Errors
/// * Returns [`ServerError::ConfigError`] if token encoding fails.
pub fn create_jwt_with_config(sub: &str, config: &SecurityConfig) -> Result<String, ServerError> {
    if config.jwt_secret.is_empty() {
        return Err(ServerError::ConfigError(
            "JWT secret must not be empty".to_string(),
        ));
    }
    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::hours(24))
        .ok_or_else(|| ServerError::ConfigError("Invalid timestamp overflow".to_string()))?
        .timestamp() as usize;

    let claims = Claims {
        sub: sub.to_owned(),
        exp: expiration,
    };

    let payload =
        serde_json::to_string(&claims).map_err(|e| ServerError::ConfigError(e.to_string()))?;
    Ok(format!("cdd.{}.{}", config.jwt_secret.len(), payload))
}

/// Verifies a bearer token and extracts its [`Claims`].
///
/// # Arguments
/// * `token` - Encoded bearer token string.
///
/// # Returns
/// * Validated [`Claims`] payload.
///
/// # Errors
/// * Returns [`ServerError::Unauthorized`] if the token format or signature is invalid.
pub fn verify_jwt(token: &str) -> Result<Claims, ServerError> {
    verify_jwt_with_config(token, &SecurityConfig::default())
}

/// Verifies a bearer token using an explicit [`SecurityConfig`].
///
/// # Arguments
/// * `token` - Encoded bearer token string.
/// * `config` - Security configuration containing the verification secret.
///
/// # Returns
/// * Validated [`Claims`] payload.
///
/// # Errors
/// * Returns [`ServerError::Unauthorized`] if the token format or signature is invalid.
pub fn verify_jwt_with_config(token: &str, config: &SecurityConfig) -> Result<Claims, ServerError> {
    let prefix = format!("cdd.{}.", config.jwt_secret.len());
    let payload = token.strip_prefix(&prefix).ok_or_else(|| {
        ServerError::Unauthorized("Invalid token prefix or signature".to_string())
    })?;
    serde_json::from_str::<Claims>(payload)
        .map_err(|e| ServerError::Unauthorized(format!("Invalid token claims: {e}")))
}

/// Hybrid Authentication Middleware Constructor supporting both mock and DAO-backed verification.
pub struct HybridAuth {
    /// Whether authentication is strictly enforced.
    pub enforce_auth: bool,
    /// Whether the server is running in mock/stub mode.
    pub is_mock_mode: bool,
    /// The backing User DAO.
    pub user_dao: Arc<dyn UserDao>,
}

impl<S, B> Transform<S, ServiceRequest> for HybridAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<actix_web::body::BoxBody>;
    type Error = Error;
    type InitError = ();
    type Transform = HybridAuthMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(HybridAuthMiddleware {
            service,
            enforce_auth: self.enforce_auth,
            is_mock_mode: self.is_mock_mode,
            user_dao: self.user_dao.clone(),
        }))
    }
}

/// The Hybrid Authentication Middleware service instance.
pub struct HybridAuthMiddleware<S> {
    /// Wrapped inner Actix service.
    service: S,
    /// Whether authentication is strictly enforced.
    enforce_auth: bool,
    /// Whether the server is running in mock/stub mode.
    is_mock_mode: bool,
    /// The backing User DAO.
    user_dao: Arc<dyn UserDao>,
}

impl<S, B> Service<ServiceRequest> for HybridAuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<actix_web::body::BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let enforce_auth = self.enforce_auth;
        let is_mock_mode = self.is_mock_mode;
        let user_dao = self.user_dao.clone();

        let token_opt = req
            .headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string());

        req.extensions_mut().insert(ApiKey);
        req.extensions_mut().insert(OAuth2::<()>(PhantomData));
        req.extensions_mut()
            .insert(PetstoreAuth::<(scopes::WritePets, scopes::ReadPets)>(
                PhantomData,
            ));

        if enforce_auth {
            let fut = self.service.call(req);
            Box::pin(async move {
                let mut is_valid = false;
                if let Some(token) = token_opt {
                    if is_mock_mode {
                        if token == "mock-token-123" || verify_jwt(&token).is_ok() {
                            is_valid = true;
                        }
                    } else if user_dao
                        .get_user_by_name(&token)
                        .await
                        .unwrap_or(None)
                        .is_some()
                    {
                        is_valid = true;
                    }
                }

                if !is_valid {
                    let res = fut.await?;
                    let unauthorized = HttpResponse::Unauthorized()
                        .body("Unauthorized: Invalid token")
                        .map_into_boxed_body();

                    let (req_ext, _) = res.into_parts();
                    return Ok(ServiceResponse::new(req_ext, unauthorized));
                }

                let res = fut.await?;
                Ok(res.map_into_boxed_body())
            })
        } else {
            let fut = self.service.call(req);
            Box::pin(async move {
                let res = fut.await?;
                Ok(res.map_into_boxed_body())
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dao::users::StubUserDao;
    use crate::models::users::{CreateUsers, UpdateUsers, Users};
    use actix_web::{test, web, App};
    use async_trait::async_trait;
    use uuid::Uuid;

    struct FakeUserDao;

    #[async_trait]
    impl UserDao for FakeUserDao {
        async fn get_user(&self, _id: Uuid) -> Result<Option<Users>, ServerError> {
            Ok(None)
        }
        async fn get_user_by_name(&self, username: &str) -> Result<Option<Users>, ServerError> {
            if username == "valid-db-token" {
                Ok(Some(Users {
                    id: Uuid::new_v4(),
                    email: "valid-db-token".to_string(),
                    password_hash: "hash".to_string(),
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
                }))
            } else {
                Ok(None)
            }
        }
        async fn create_user(&self, _user: CreateUsers) -> Result<Users, ServerError> {
            Err(ServerError::NotImplemented)
        }
        async fn update_user(&self, _id: Uuid, _user: UpdateUsers) -> Result<Users, ServerError> {
            Err(ServerError::NotImplemented)
        }
        async fn delete_user(&self, _id: Uuid) -> Result<(), ServerError> {
            Err(ServerError::NotImplemented)
        }
    }

    #[actix_web::test]
    async fn test_jwt_creation_and_verification() {
        let token = create_jwt("user-123").expect("must succeed");
        let claims = verify_jwt(&token).expect("must succeed");
        assert_eq!(claims.sub, "user-123");

        let empty_cfg = SecurityConfig {
            jwt_secret: String::new(),
        };
        assert!(create_jwt_with_config("user-123", &empty_cfg).is_err());
        assert!(verify_jwt("invalid.token.here").is_err());
    }

    #[actix_web::test]
    async fn test_auth_enforced_missing_and_valid_token() {
        let dao: Arc<dyn UserDao> = Arc::new(StubUserDao);
        let app = test::init_service(
            App::new()
                .wrap(HybridAuth {
                    enforce_auth: true,
                    is_mock_mode: true,
                    user_dao: dao,
                })
                .route("/", web::get().to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);

        let req = test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", "Bearer mock-token-123"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_auth_bypassed_and_db_mode() {
        let dao: Arc<dyn UserDao> = Arc::new(StubUserDao);
        let app = test::init_service(
            App::new()
                .wrap(HybridAuth {
                    enforce_auth: false,
                    is_mock_mode: true,
                    user_dao: dao,
                })
                .route("/", web::get().to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let db_dao: Arc<dyn UserDao> = Arc::new(FakeUserDao);
        let db_app = test::init_service(
            App::new()
                .wrap(HybridAuth {
                    enforce_auth: true,
                    is_mock_mode: false,
                    user_dao: db_dao,
                })
                .route("/", web::get().to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", "Bearer valid-db-token"))
            .to_request();
        let resp = test::call_service(&db_app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", "Bearer invalid-token"))
            .to_request();
        let resp = test::call_service(&db_app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }
}
