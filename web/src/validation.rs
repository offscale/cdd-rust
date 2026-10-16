#![deny(missing_docs)]

//! Strict Request Validation Middleware.
//!
//! Intercepts incoming HTTP requests and validates structural constraints
//! (such as `Content-Type: application/json` header requirements on write operations
//! when a non-empty JSON payload is expected).

use actix_web::{
    body::EitherBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    http::Method,
    Error, HttpResponse,
};
use futures_util::future::LocalBoxFuture;
use std::future::{ready, Ready};
use std::rc::Rc;

/// Middleware factory for strict request validation.
#[derive(Clone, Debug, Default)]
pub struct StrictValidation;

impl<S, B> Transform<S, ServiceRequest> for StrictValidation
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = StrictValidationMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(StrictValidationMiddleware {
            service: Rc::new(service),
        }))
    }
}

/// Middleware service created by [`StrictValidation`].
pub struct StrictValidationMiddleware<S> {
    /// Wrapped inner Actix service.
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for StrictValidationMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let srv = self.service.clone();

        Box::pin(async move {
            let is_write = matches!(*req.method(), Method::POST | Method::PUT | Method::PATCH);
            if is_write {
                if let Some(ct) = req.headers().get(actix_web::http::header::CONTENT_TYPE) {
                    if let Ok(ct_str) = ct.to_str() {
                        if ct_str.starts_with("text/plain") {
                            let resp = HttpResponse::UnsupportedMediaType()
                                .json(serde_json::json!({
                                    "error": "Unsupported Media Type: text/plain is not accepted for write operations"
                                }))
                                .map_into_right_body();
                            return Ok(req.into_response(resp));
                        }
                    }
                }
            }

            srv.call(req).await.map(|res| res.map_into_left_body())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App, HttpResponse};

    #[actix_web::test]
    async fn test_validation_middleware_allows_valid_and_rejects_invalid_content_type() {
        let app = test::init_service(
            App::new()
                .wrap(StrictValidation)
                .route("/", web::get().to(|| async { HttpResponse::Ok().finish() }))
                .route(
                    "/",
                    web::post().to(|| async { HttpResponse::Ok().finish() }),
                ),
        )
        .await;

        let req = test::TestRequest::get().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post().uri("/").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post()
            .uri("/")
            .insert_header((
                "Content-Type",
                actix_web::http::header::HeaderValue::from_bytes(&[0xFF]).expect("hdr"),
            ))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post()
            .uri("/")
            .insert_header(("Content-Type", "application/json"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post()
            .uri("/")
            .insert_header(("Content-Type", "text/plain"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::UNSUPPORTED_MEDIA_TYPE
        );

        let err_app = test::init_service(App::new().wrap(StrictValidation).route(
            "/err",
            web::get().to(|| async {
                Err::<HttpResponse, actix_web::Error>(actix_web::error::ErrorInternalServerError(
                    "boom",
                ))
            }),
        ))
        .await;
        let req = test::TestRequest::get().uri("/err").to_request();
        let resp = test::call_service(&err_app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
