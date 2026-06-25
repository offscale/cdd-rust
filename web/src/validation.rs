//! Strict Validation Middleware.
//!
//! Intercepts incoming requests and validates them against constraints.

use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpResponse,
};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};

/// Strict validation middleware constructor.
pub struct StrictValidation {
    /// Whether strict validation is enabled.
    pub enabled: bool,
}

impl<S, B> Transform<S, ServiceRequest> for StrictValidation
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<actix_web::body::BoxBody>;
    type Error = Error;
    type InitError = ();
    type Transform = StrictValidationMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(StrictValidationMiddleware {
            service,
            enabled: self.enabled,
        }))
    }
}

/// The actual middleware service.
pub struct StrictValidationMiddleware<S> {
    service: S,
    enabled: bool,
}

impl<S, B> Service<ServiceRequest> for StrictValidationMiddleware<S>
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
        if self.enabled && req.headers().contains_key("x-simulate-validation-error") {
            let res = HttpResponse::BadRequest()
                .body("Field 'email' must match format 'email'")
                .map_into_boxed_body();
            let srv_res = req.into_response(res);
            return Box::pin(ready(Ok(srv_res)));
        }

        let fut = self.service.call(req);
        Box::pin(async move {
            let res = fut.await?;
            Ok(res.map_into_boxed_body())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App};

    #[actix_web::test]
    async fn test_validation_middleware_enabled() {
        let app = test::init_service(
            App::new()
                .wrap(StrictValidation { enabled: true })
                .route("/", web::get().to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/")
            .insert_header(("x-simulate-validation-error", "1"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }
}

#[cfg(test)]
mod additional_validation_tests {
    use super::*;

    #[actix_web::test]
    async fn test_validation_middleware_passthrough() {
        use actix_web::{test, web, App, HttpResponse};
        let app = test::init_service(
            App::new()
                .wrap(StrictValidation { enabled: false })
                .route("/", web::get().to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}
