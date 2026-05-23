#![allow(unused_variables)]
#![allow(unused_imports)]

use crate::models::*;
use crate::security;
use actix_multipart::Multipart;
use actix_web::{web, HttpResponse, Responder};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

/// Returns pet inventories by status
///
/// Returns a map of status codes to quantities
pub async fn get_inventory(api_key: web::ReqData<security::ApiKey>) -> impl Responder {
    HttpResponse::NotImplemented().finish()
}

/// Query parameters for `place_order`.
#[derive(Debug, Clone, Deserialize)]
pub struct PlaceOrderQuery {
    /// order placed for purchasing the pet
    pub body: Order,
}

/// Place an order for a pet
///
pub async fn place_order(query: web::Query<PlaceOrderQuery>) -> impl Responder {
    HttpResponse::NotImplemented().finish()
}

/// Find purchase order by ID
///
/// For valid response try integer IDs with value >= 1 and <= 10. Other values will generated exceptions
pub async fn get_order_by_id(order_id: web::Path<i64>) -> impl Responder {
    HttpResponse::NotImplemented().finish()
}

/// Delete purchase order by ID
///
/// For valid response try integer IDs with positive integer value. Negative or non-integer values will generate API errors
pub async fn delete_order(order_id: web::Path<i64>) -> impl Responder {
    HttpResponse::NotImplemented().finish()
}
