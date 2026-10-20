#![deny(missing_docs)]

//! Store endpoint parameter models.

use serde::{Deserialize, Serialize};

/// Path parameter model for store order operations (`/store/order/{orderId}`).
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct OrderIdParam {
    /// Numeric ID of the order.
    #[serde(alias = "orderId")]
    pub order_id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App, HttpResponse};

    #[actix_web::test]
    async fn test_order_id_param_path_extraction() {
        let app = test::init_service(App::new().route(
            "/store/order/{orderId}",
            web::get().to(|p: web::Path<OrderIdParam>| async move {
                HttpResponse::Ok().json(p.into_inner())
            }),
        ))
        .await;

        let req = test::TestRequest::get().uri("/store/order/42").to_request();
        let extracted: OrderIdParam = test::call_and_read_body_json(&app, req).await;
        assert_eq!(extracted, OrderIdParam { order_id: 42 });
    }
}
