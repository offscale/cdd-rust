#![deny(missing_docs)]

//! Store inventory and order HTTP handlers backed by thread-safe in-memory state.

use crate::error::ServerError;
use actix_web::{web, HttpResponse};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// Thread-safe in-memory store for orders and status inventory counts.
#[derive(Default)]
pub struct OrderStore {
    /// Mutex-protected map from order ID to JSON representation.
    pub orders: Mutex<HashMap<i64, Value>>,
}

/// Returns pet inventories aggregated by status (`GET /store/inventory`).
///
/// # Arguments
/// * `store` - Optional shared [`OrderStore`] state.
///
/// # Returns
/// * `200 OK` with a JSON object mapping status names to quantities.
///
/// # Errors
/// * Returns [`ServerError::SyncError`] if the internal mutex is poisoned.
pub async fn get_inventory(
    store: Option<web::Data<OrderStore>>,
) -> Result<HttpResponse, ServerError> {
    let mut counts: HashMap<String, i64> = HashMap::new();
    if let Some(store) = store {
        let orders = store
            .orders
            .lock()
            .map_err(|_| ServerError::SyncError("OrderStore lock poisoned".to_string()))?;
        for order in orders.values() {
            let status = order
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("placed")
                .to_string();
            let qty = order.get("quantity").and_then(|v| v.as_i64()).unwrap_or(1);
            *counts.entry(status).or_insert(0) += qty;
        }
    }
    Ok(HttpResponse::Ok().json(counts))
}

/// Places a new pet order (`POST /store/order`).
///
/// # Arguments
/// * `body` - Optional JSON order payload.
/// * `store` - Optional shared [`OrderStore`] state.
///
/// # Returns
/// * `200 OK` with the stored order JSON.
///
/// # Errors
/// * Returns [`ServerError::SyncError`] if the internal mutex is poisoned.
pub async fn place_order(
    body: Option<web::Json<Value>>,
    store: Option<web::Data<OrderStore>>,
) -> Result<HttpResponse, ServerError> {
    let payload = body
        .map(|b| b.into_inner())
        .unwrap_or_else(|| serde_json::json!({"id": 1, "status": "placed", "quantity": 1}));
    let id = payload.get("id").and_then(|v| v.as_i64()).unwrap_or(1);

    if let Some(store) = store {
        let mut orders = store
            .orders
            .lock()
            .map_err(|_| ServerError::SyncError("OrderStore lock poisoned".to_string()))?;
        orders.insert(id, payload.clone());
    }

    Ok(HttpResponse::Ok().json(payload))
}

/// Retrieves a purchase order by its ID (`GET /store/order/{orderId}`).
///
/// # Arguments
/// * `order_id` - Path parameter specifying the order's numeric ID.
/// * `store` - Optional shared [`OrderStore`] state.
///
/// # Returns
/// * `200 OK` with the order JSON if found, or `404 Not Found` if missing in a configured store.
///
/// # Errors
/// * Returns [`ServerError::SyncError`] if the internal mutex is poisoned.
pub async fn get_order_by_id(
    order_id: web::Path<i64>,
    store: Option<web::Data<OrderStore>>,
) -> Result<HttpResponse, ServerError> {
    let id = order_id.into_inner();
    if let Some(store) = store {
        let orders = store
            .orders
            .lock()
            .map_err(|_| ServerError::SyncError("OrderStore lock poisoned".to_string()))?;
        if let Some(order) = orders.get(&id) {
            return Ok(HttpResponse::Ok().json(order));
        }
        return Ok(HttpResponse::NotFound().finish());
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({"id": id, "status": "placed"})))
}

/// Deletes a purchase order by its ID (`DELETE /store/order/{orderId}`).
///
/// # Arguments
/// * `order_id` - Path parameter specifying the order's numeric ID.
/// * `store` - Optional shared [`OrderStore`] state.
///
/// # Returns
/// * `200 OK` if deleted, or `404 Not Found` if missing in a configured store.
///
/// # Errors
/// * Returns [`ServerError::SyncError`] if the internal mutex is poisoned.
pub async fn delete_order(
    order_id: web::Path<i64>,
    store: Option<web::Data<OrderStore>>,
) -> Result<HttpResponse, ServerError> {
    let id = order_id.into_inner();
    if let Some(store) = store {
        let mut orders = store
            .orders
            .lock()
            .map_err(|_| ServerError::SyncError("OrderStore lock poisoned".to_string()))?;
        if orders.remove(&id).is_some() {
            return Ok(HttpResponse::Ok().finish());
        }
        return Ok(HttpResponse::NotFound().finish());
    }
    Ok(HttpResponse::Ok().finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_store_handlers_stateful_and_stateless() {
        let store = web::Data::new(OrderStore::default());
        let app = test::init_service(
            App::new()
                .app_data(store.clone())
                .route("/store/inventory", web::get().to(get_inventory))
                .route("/store/order", web::post().to(place_order))
                .route("/store/order/{orderId}", web::get().to(get_order_by_id))
                .route("/store/order/{orderId}", web::delete().to(delete_order)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/store/order")
            .set_json(serde_json::json!({"id": 10, "status": "approved", "quantity": 3}))
            .to_request();
        let created: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(created["id"], 10);

        let req = test::TestRequest::get()
            .uri("/store/inventory")
            .to_request();
        let inv: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(inv["approved"], 3);

        let req = test::TestRequest::get().uri("/store/order/10").to_request();
        let fetched: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(fetched["status"], "approved");

        let req = test::TestRequest::get().uri("/store/order/99").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);

        let req = test::TestRequest::delete()
            .uri("/store/order/10")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::delete()
            .uri("/store/order/10")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);

        let store_clone = store.clone();
        let _ = std::thread::spawn(move || {
            let _guard = store_clone.orders.lock().expect("lock");
            panic!("poison order store");
        })
        .join();

        let req = test::TestRequest::get()
            .uri("/store/inventory")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        let req = test::TestRequest::post()
            .uri("/store/order")
            .set_json(serde_json::json!({"id": 1}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        let req = test::TestRequest::get().uri("/store/order/1").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        let req = test::TestRequest::delete()
            .uri("/store/order/1")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[actix_web::test]
    async fn test_store_handlers_without_app_data() {
        let app = test::init_service(
            App::new()
                .route("/store/inventory", web::get().to(get_inventory))
                .route("/store/order", web::post().to(place_order))
                .route("/store/order/{orderId}", web::get().to(get_order_by_id))
                .route("/store/order/{orderId}", web::delete().to(delete_order)),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/store/inventory")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post().uri("/store/order").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::get().uri("/store/order/1").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::delete()
            .uri("/store/order/1")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}
