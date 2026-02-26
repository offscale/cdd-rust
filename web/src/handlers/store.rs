use actix_web::{web, HttpResponse, Responder};

pub async fn get_inventory() -> impl Responder {
    HttpResponse::Ok().finish()
}
pub async fn place_order() -> impl Responder {
    HttpResponse::Ok().finish()
}
pub async fn get_order_by_id() -> impl Responder {
    HttpResponse::Ok().finish()
}
pub async fn delete_order() -> impl Responder {
    HttpResponse::Ok().finish()
}
