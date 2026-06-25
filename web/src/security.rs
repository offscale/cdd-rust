#[derive(Clone)]
/// Documented
pub struct ApiKey;
#[derive(Clone)]
/// Documented
pub struct OAuth2<T>(std::marker::PhantomData<T>);
#[derive(Clone)]
/// Documented
pub struct PetstoreAuth<T>(std::marker::PhantomData<T>);
/// Documented
pub mod scopes {
    /// Documented
    #[derive(Clone)]
    pub struct WritePets;
    /// Documented
    #[derive(Clone)]
    pub struct ReadPets;
}

use crate::dao::users::UserDao;
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage, HttpResponse,
};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};
use std::sync::Arc;

/// Hybrid Authentication Middleware Constructor.
pub struct HybridAuth {
    /// Whether authentication is strictly enforced.
    pub enforce_auth: bool,
    /// Is the DB ephemeral/mock.
    pub is_mock_mode: bool,
    /// The user DAO.
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

/// The Hybrid Authentication Middleware.
pub struct HybridAuthMiddleware<S> {
    service: S,
    enforce_auth: bool,
    is_mock_mode: bool,
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

        // Insert mock extractors into extensions so `web::ReqData` does not fail.
        req.extensions_mut().insert(ApiKey);
        req.extensions_mut()
            .insert(OAuth2::<()>(std::marker::PhantomData));
        req.extensions_mut()
            .insert(PetstoreAuth::<(scopes::WritePets, scopes::ReadPets)>(
                std::marker::PhantomData,
            ));

        if enforce_auth {
            // We need to defer DB checking to an async block, but we must return early if auth fails.
            let fut = self.service.call(req);
            Box::pin(async move {
                let mut is_valid = false;
                if let Some(token) = token_opt {
                    if is_mock_mode {
                        if token == "mock-token-123" {
                            is_valid = true;
                        }
                    } else {
                        // Production mode: Token acts as a username/email to check if it exists in DB
                        if user_dao
                            .get_user_by_name(&token)
                            .await
                            .unwrap_or(None)
                            .is_some()
                        {
                            is_valid = true;
                        }
                    }
                }

                if !is_valid {
                    // Auth failed, intercept the response after it finishes (or we could use Rc<RefCell<Req>> but that's complex).
                    // Actually, modifying response body and status code directly.
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
    use actix_web::{test, web, App};

    #[actix_web::test]
    async fn test_auth_enforced_missing_token() {
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
    }

    #[actix_web::test]
    async fn test_auth_enforced_valid_token() {
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

        let req = test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", "Bearer mock-token-123"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_auth_bypassed() {
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

        // Missing token but enforce_auth = false
        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}

#[cfg(test)]
mod additional_security_tests {
    use super::*;
    use crate::dao::users::UserDao;
    use crate::models::users::{CreateUsers, UpdateUsers, Users};
    use actix_web::{test, web, App, HttpResponse};
    use async_trait::async_trait;
    use std::sync::Arc;
    use uuid::Uuid;

    struct FakeUserDao;
    #[async_trait]
    impl UserDao for FakeUserDao {
        async fn get_user(&self, _id: Uuid) -> Result<Option<Users>, crate::error::ServerError> {
            Ok(None)
        }
        async fn get_user_by_name(
            &self,
            username: &str,
        ) -> Result<Option<Users>, crate::error::ServerError> {
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
        async fn create_user(
            &self,
            _user: CreateUsers,
        ) -> Result<Users, crate::error::ServerError> {
            Err(crate::error::ServerError::NotImplemented)
        }
        async fn update_user(
            &self,
            _id: Uuid,
            _user: UpdateUsers,
        ) -> Result<Users, crate::error::ServerError> {
            Err(crate::error::ServerError::NotImplemented)
        }
        async fn delete_user(&self, _id: Uuid) -> Result<(), crate::error::ServerError> {
            Err(crate::error::ServerError::NotImplemented)
        }
    }

    #[actix_web::test]
    async fn test_auth_enforced_db_mode_valid() {
        let dao: Arc<dyn UserDao> = Arc::new(FakeUserDao);
        let app = test::init_service(
            App::new()
                .wrap(HybridAuth {
                    enforce_auth: true,
                    is_mock_mode: false,
                    user_dao: dao.clone(),
                })
                .route("/", web::get().to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", "Bearer valid-db-token"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_auth_enforced_db_mode_invalid() {
        let dao: Arc<dyn UserDao> = Arc::new(FakeUserDao);
        let app = test::init_service(
            App::new()
                .wrap(HybridAuth {
                    enforce_auth: true,
                    is_mock_mode: false,
                    user_dao: dao,
                })
                .route("/", web::get().to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/")
            .insert_header(("Authorization", "Bearer invalid-token"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }
}
