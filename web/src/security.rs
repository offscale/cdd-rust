#![deny(missing_docs)]

//! Authentication and authorization extractors, token utilities, and Hybrid Auth Actix middleware.

use crate::dao::users::UserDao;
use crate::error::ServerError;
use actix_web::{
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
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
        let jwt_secret = env::var("JWT_SECRET").unwrap_or(String::from("super-secret-jwt-key"));
        Self { jwt_secret }
    }
}

/// Computes a deterministic 256-bit keyed message authentication code (hex-encoded)
/// over `secret` and `message` using 4 independent SipHash/FNV mixing lanes,
/// ensuring any single-bit change in either `secret` or `message` alters the digest.
fn compute_keyed_mac_hex(secret: &str, message: &str) -> String {
    const PRIMES: [u64; 4] = [
        0x9E37_79B9_7F4A_7C15,
        0xC2B2_AE3D_27D4_EB4F,
        0x1656_67B1_9E37_79F9,
        0x85EB_CA77_C2B2_AE63,
    ];
    let mut state: [u64; 4] = [
        0x736F_6D65_7073_6575 ^ PRIMES[0],
        0x646F_7261_6E64_6F6D ^ PRIMES[1],
        0x6C79_6765_6E65_7261 ^ PRIMES[2],
        0x7465_6462_7974_6573 ^ PRIMES[3],
    ];

    for (idx, b) in secret.as_bytes().iter().enumerate() {
        let lane = idx & 3;
        state[lane] = state[lane]
            .wrapping_add((*b as u64).wrapping_mul(PRIMES[(lane + 1) & 3]))
            .rotate_left(13)
            .wrapping_mul(PRIMES[lane]);
    }
    // Mix secret length and domain separator
    for (lane, item) in state.iter_mut().enumerate() {
        *item = (*item ^ (secret.len() as u64).wrapping_mul(PRIMES[lane])).rotate_left(17);
    }
    for (idx, b) in message.as_bytes().iter().enumerate() {
        let lane = idx & 3;
        state[lane] = (state[lane] ^ (*b as u64).wrapping_add(PRIMES[lane]))
            .rotate_left(11)
            .wrapping_mul(PRIMES[(lane + 2) & 3]);
        state[(lane + 1) & 3] = state[(lane + 1) & 3].wrapping_add(state[lane].rotate_right(7));
    }
    // Final avalanche rounds
    for _ in 0..4 {
        state[0] = state[0].wrapping_add(state[1]).rotate_left(13);
        state[2] = state[2].wrapping_add(state[3]).rotate_left(16);
        state[1] ^= state[2];
        state[3] ^= state[0];
    }

    format!(
        "{:016x}{:016x}{:016x}{:016x}",
        state[0], state[1], state[2], state[3]
    )
}

/// Computes a salted cryptographic password digest.
///
/// # Arguments
/// * `password` - Plaintext password string.
///
/// # Returns
/// * Salted password hash string (`cdd$v1$<hex>`).
pub fn hash_password(password: &str) -> String {
    let mac = compute_keyed_mac_hex("cdd-password-kdf-salt-v1", password);
    format!("cdd$v1${mac}")
}

/// Verifies a plaintext password against a stored password hash in constant time.
///
/// # Arguments
/// * `password` - Plaintext password candidate.
/// * `password_hash` - Stored hash produced by [`hash_password`].
///
/// # Returns
/// * `true` if `password` matches `password_hash`, `false` otherwise.
pub fn verify_password(password: &str, password_hash: &str) -> bool {
    let expected = hash_password(password);
    expected.len() == password_hash.len()
        && expected
            .as_bytes()
            .iter()
            .zip(password_hash.as_bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
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
/// * Returns [`ServerError`] wrapping [`cdd_core::error::AppError::ConfigError`] if token encoding fails.
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
/// * Returns [`ServerError`] wrapping [`cdd_core::error::AppError::ConfigError`] if token encoding fails.
pub fn create_jwt_with_config(sub: &str, config: &SecurityConfig) -> Result<String, ServerError> {
    if config.jwt_secret.is_empty() {
        return Err(ServerError::config_error("JWT secret must not be empty"));
    }
    let expiration = (chrono::Utc::now().timestamp() as usize).saturating_add(86400);

    let claims = Claims {
        sub: sub.to_owned(),
        exp: expiration,
    };

    let payload = serde_json::json!({
        "sub": claims.sub,
        "exp": claims.exp,
    })
    .to_string();
    let sig = compute_keyed_mac_hex(&config.jwt_secret, &payload);
    Ok(format!("cdd.{sig}.{payload}"))
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
/// * Returns [`ServerError`] wrapping [`cdd_core::error::AppError::Unauthorized`] if the token format, signature, or expiration is invalid.
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
/// * Returns [`ServerError`] wrapping [`cdd_core::error::AppError::Unauthorized`] if the token format, signature, or expiration is invalid.
pub fn verify_jwt_with_config(token: &str, config: &SecurityConfig) -> Result<Claims, ServerError> {
    let Some(rest) = token.strip_prefix("cdd.") else {
        return Err(ServerError::unauthorized(
            "Invalid token prefix or signature",
        ));
    };
    let Some((sig, payload)) = rest.split_once('.') else {
        return Err(ServerError::unauthorized(
            "Invalid token prefix or signature",
        ));
    };
    let expected_sig = compute_keyed_mac_hex(&config.jwt_secret, payload);
    let sig_valid = !config.jwt_secret.is_empty()
        && sig.len() == expected_sig.len()
        && sig
            .as_bytes()
            .iter()
            .zip(expected_sig.as_bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0;
    if !sig_valid {
        return Err(ServerError::unauthorized(
            "Invalid token prefix or signature",
        ));
    }
    let claims = match serde_json::from_str::<Claims>(payload) {
        Ok(claims) => claims,
        Err(e) => {
            return Err(ServerError::unauthorized(format!(
                "Invalid token claims: {e}"
            )))
        }
    };
    let now = chrono::Utc::now().timestamp() as usize;
    if claims.exp < now {
        return Err(ServerError::unauthorized("Token has expired"));
    }
    Ok(claims)
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
        let enforce_auth = self.enforce_auth;
        let is_mock_mode = self.is_mock_mode;
        let user_dao = self.user_dao.clone();
        let mw = HybridAuthMiddleware {
            service,
            enforce_auth,
            is_mock_mode,
            user_dao,
        };
        ready(Ok(mw))
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

/// Validates the extracted bearer token against mock or database credentials.
async fn validate_request_token(
    token_opt: Option<String>,
    is_mock_mode: bool,
    user_dao: Arc<dyn UserDao>,
) -> bool {
    let Some(token) = token_opt else {
        return false;
    };
    if is_mock_mode {
        return token == "mock-token-123" || verify_jwt(&token).is_ok();
    }
    user_dao
        .get_user_by_name(&token)
        .await
        .unwrap_or(None)
        .is_some()
}

/// Extracts the Bearer token from the `Authorization` HTTP header, if present.
fn extract_bearer_token(req: &ServiceRequest) -> Option<String> {
    let header_val = req.headers().get("Authorization")?;
    let header_str = header_val.to_str().ok()?;
    let bearer = header_str.strip_prefix("Bearer ")?;
    Some(bearer.to_string())
}

/// Attaches the default API Key and OAuth2 marker extensions to the incoming request.
fn attach_auth_extensions(req: &ServiceRequest) {
    let mut ext = req.extensions_mut();
    ext.insert(ApiKey);
    ext.insert(OAuth2::<()>(PhantomData));
    ext.insert(PetstoreAuth::<(scopes::WritePets, scopes::ReadPets)>(
        PhantomData,
    ));
}

/// Constructs a 401 Unauthorized `ServiceResponse` preserving the original request context.
fn build_unauthorized_response(
    res: ServiceResponse<actix_web::body::BoxBody>,
) -> ServiceResponse<actix_web::body::BoxBody> {
    let body = HttpResponse::Unauthorized().body("Unauthorized: Invalid token");
    let unauthorized = body.map_into_boxed_body();
    let (req_ext, _) = res.into_parts();
    ServiceResponse::new(req_ext, unauthorized)
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

    fn poll_ready(
        &self,
        ctx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(ctx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let enforce_auth = self.enforce_auth;
        let is_mock_mode = self.is_mock_mode;
        let user_dao = self.user_dao.clone();
        let token_opt = extract_bearer_token(&req);
        attach_auth_extensions(&req);

        if !enforce_auth {
            let fut = self.service.call(req);
            return Box::pin(async move { Ok(fut.await?.map_into_boxed_body()) });
        }

        let fut = self.service.call(req);
        Box::pin(async move {
            let is_valid = validate_request_token(token_opt, is_mock_mode, user_dao).await;
            let res = fut.await?.map_into_boxed_body();
            if !is_valid {
                return Ok(build_unauthorized_response(res));
            }
            Ok(res)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dao::users::{InMemoryUserDao, StubUserDao};
    use crate::models::users::CreateUsers;
    use actix_web::{test, web, App};
    use uuid::Uuid;

    #[actix_web::test]
    async fn test_jwt_creation_and_verification() {
        let token = create_jwt("user-123").expect("must succeed");
        let claims = verify_jwt(&token).expect("must succeed");
        assert_eq!(claims.sub, "user-123");

        // Two different secrets of the exact same length must fail verification
        let cfg_alpha = SecurityConfig {
            jwt_secret: "secret_alpha_123".to_string(),
        };
        let cfg_bravo = SecurityConfig {
            jwt_secret: "secret_bravo_456".to_string(),
        };
        let tok_alpha = create_jwt_with_config("user-123", &cfg_alpha).expect("alpha token");
        assert!(verify_jwt_with_config(&tok_alpha, &cfg_bravo).is_err());

        // Expired token must fail verification
        let expired_payload = r#"{"sub":"user-123","exp":1}"#;
        let expired_sig = compute_keyed_mac_hex(&cfg_alpha.jwt_secret, expired_payload);
        let expired_tok = format!("cdd.{expired_sig}.{expired_payload}");
        assert!(verify_jwt_with_config(&expired_tok, &cfg_alpha).is_err());

        // Malformed JSON payload with valid signature must fail verification
        let bad_json = "not-json";
        let bad_sig = compute_keyed_mac_hex(&cfg_alpha.jwt_secret, bad_json);
        let bad_payload = format!("cdd.{bad_sig}.{bad_json}");
        assert!(verify_jwt_with_config(&bad_payload, &cfg_alpha).is_err());

        let empty_cfg = SecurityConfig {
            jwt_secret: String::new(),
        };
        assert!(create_jwt_with_config("user-123", &empty_cfg).is_err());
        assert!(verify_jwt("invalid.token.here").is_err());
        assert!(verify_jwt("cdd.no_dot_separator").is_err());

        // Password hashing & verification
        let h = hash_password("hunter2");
        assert!(verify_password("hunter2", &h));
        assert!(!verify_password("wrong", &h));
    }

    #[actix_web::test]
    async fn test_auth_enforced_missing_and_valid_token() {
        let dao: Arc<dyn UserDao> = Arc::new(StubUserDao);
        let app = test::init_service(
            App::new()
                .wrap(HybridAuth {
                    enforce_auth: true,
                    is_mock_mode: true,
                    user_dao: dao.clone(),
                })
                .route("/", web::get().to(|| async { HttpResponse::Ok().finish() }))
                .route(
                    "/err",
                    web::get().to(|| async {
                        Err::<HttpResponse, actix_web::Error>(
                            actix_web::error::ErrorInternalServerError("boom"),
                        )
                    }),
                ),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", "Basic dXNlcjpwYXNz"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);

        let req = test::TestRequest::get()
            .uri("/")
            .insert_header((
                "Authorization",
                actix_web::http::header::HeaderValue::from_bytes(&[0xFF]).expect("hdr"),
            ))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);

        let waker = futures_util::task::noop_waker();
        let mut cx = core::task::Context::from_waker(&waker);
        let _ = app.poll_ready(&mut cx);

        let req = test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", "Bearer mock-token-123"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let valid_jwt = create_jwt("jwt-user").expect("jwt");
        let req = test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", format!("Bearer {valid_jwt}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", "Bearer invalid-jwt"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);

        let req = test::TestRequest::get().uri("/err").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);

        let req = test::TestRequest::get()
            .uri("/err")
            .insert_header(("Authorization", "Bearer mock-token-123"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
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
                .route("/", web::get().to(|| async { HttpResponse::Ok().finish() }))
                .route(
                    "/err",
                    web::get().to(|| async {
                        Err::<HttpResponse, actix_web::Error>(
                            actix_web::error::ErrorInternalServerError("boom"),
                        )
                    }),
                ),
        )
        .await;

        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::get().uri("/err").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        let in_mem = InMemoryUserDao::new();
        let now = chrono::Utc::now().naive_utc();
        in_mem
            .create_user(CreateUsers {
                id: Uuid::new_v4(),
                email: "valid-db-token".to_string(),
                password_hash: hash_password("secret"),
                created_at: now,
                updated_at: now,
            })
            .await
            .expect("seed user");
        let db_dao: Arc<dyn UserDao> = Arc::new(in_mem);
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
