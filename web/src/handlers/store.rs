use actix_web::{HttpResponse, Responder};

/// Documented
pub async fn get_inventory() -> impl Responder {
    HttpResponse::Ok().finish()
}
/// Documented
pub async fn place_order() -> impl Responder {
    HttpResponse::Ok().finish()
}
/// Documented
pub async fn get_order_by_id() -> impl Responder {
    HttpResponse::Ok().finish()
}
/// Documented
pub async fn delete_order() -> impl Responder {
    HttpResponse::Ok().finish()
}
#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App};

    #[actix_web::test]
    async fn test_store_handlers() {
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
