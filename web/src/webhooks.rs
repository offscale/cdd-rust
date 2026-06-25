//! Webhooks & Callbacks Support.
//!
//! Provides an administrative API to trigger webhooks manually.

use actix_web::{web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

/// Payload sent to the webhook receiver.
#[derive(Serialize, Clone)]
pub struct WebhookPayload {
    /// Name of the triggered webhook.
    pub name: String,
    /// Arbitrary data.
    pub data: String,
}

/// Request query for triggering a webhook.
#[derive(Deserialize)]
pub struct TriggerQuery {
    /// The target URL to dispatch the webhook to.
    pub target_url: String,
}

/// Administrative endpoint to trigger a webhook.
///
/// Dispatches a predefined payload to the provided `target_url`.
pub async fn trigger_webhook(
    path: web::Path<String>,
    query: web::Query<TriggerQuery>,
) -> impl Responder {
    let webhook_name = path.into_inner();
    let target = query.target_url.clone();

    let client = reqwest::Client::new();
    let payload = WebhookPayload {
        name: webhook_name.clone(),
        data: "mock data".into(),
    };

    match client.post(&target).json(&payload).send().await {
        Ok(res) if res.status().is_success() => HttpResponse::Ok().json(serde_json::json!({
            "status": "success",
            "webhook": webhook_name,
            "dispatched_to": target
        })),
        Ok(res) => HttpResponse::BadGateway().json(serde_json::json!({
            "status": "failed",
            "reason": format!("Receiver returned status {}", res.status())
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "status": "error",
            "reason": e.to_string()
        })),
    }
}

/// Configuration for webhook endpoints.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/_mock").route(
        "/trigger-webhook/{webhook_name}",
        web::post().to(trigger_webhook),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use httpmock::prelude::*;

    #[actix_web::test]
    async fn test_trigger_webhook_success() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/receiver");
            then.status(200);
        });

        let app = test::init_service(App::new().configure(config)).await;

        let target = format!("{}/receiver", server.base_url());
        let uri = format!(
            "/_mock/trigger-webhook/my_hook?target_url={}",
            urlencoding::encode(&target)
        );

        let req = test::TestRequest::post().uri(&uri).to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
        mock.assert();
    }
}

#[cfg(test)]
mod additional_webhooks_tests {
    use super::*;
    use actix_web::{test, App};
    use httpmock::prelude::*;

    #[actix_web::test]
    async fn test_trigger_webhook_bad_gateway() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/receiver");
            then.status(500);
        });

        let app = test::init_service(App::new().configure(config)).await;

        let target = format!("{}/receiver", server.base_url());
        let uri = format!(
            "/_mock/trigger-webhook/my_hook?target_url={}",
            urlencoding::encode(&target)
        );

        let req = test::TestRequest::post().uri(&uri).to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_GATEWAY);
        mock.assert();
    }

    #[actix_web::test]
    async fn test_trigger_webhook_error() {
        let app = test::init_service(App::new().configure(config)).await;

        let target = "http://localhost:1"; // Assuming connection refused
        let uri = format!(
            "/_mock/trigger-webhook/my_hook?target_url={}",
            urlencoding::encode(target)
        );

        let req = test::TestRequest::post().uri(&uri).to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
