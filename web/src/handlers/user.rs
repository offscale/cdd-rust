use actix_web::{web, HttpResponse, Responder};

pub async fn create_user() -> impl Responder {
    HttpResponse::Ok().finish()
}
pub async fn create_users_with_list_input() -> impl Responder {
    HttpResponse::Ok().finish()
}
pub async fn login_user() -> impl Responder {
    HttpResponse::Ok().finish()
}
pub async fn logout_user() -> impl Responder {
    HttpResponse::Ok().finish()
}
pub async fn get_user_by_name() -> impl Responder {
    HttpResponse::Ok().finish()
}
pub async fn update_user() -> impl Responder {
    HttpResponse::Ok().finish()
}
pub async fn delete_user() -> impl Responder {
    HttpResponse::Ok().finish()
}
/// Creates list of users with given input array
///
pub async fn create_users_with_array_input(
    _body: web::Json<Vec<crate::models::User>>,
) -> impl Responder {
    HttpResponse::NotImplemented().finish()
}
#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_user_handlers() {
        let app = test::init_service(
            App::new()
                .route("/user", web::post().to(create_user))
                .route(
                    "/user/createWithArray",
                    web::post().to(create_users_with_list_input),
                )
                .route("/user/login", web::get().to(login_user))
                .route("/user/logout", web::get().to(logout_user))
                .route("/user/{username}", web::get().to(get_user_by_name))
                .route("/user/{username}", web::put().to(update_user))
                .route("/user/{username}", web::delete().to(delete_user))
                .route(
                    "/user/createWithArrayReal",
                    web::post().to(create_users_with_array_input),
                ),
        )
        .await;

        let req = test::TestRequest::post().uri("/user").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post()
            .uri("/user/createWithArray")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::get().uri("/user/login").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::get().uri("/user/logout").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::get().uri("/user/john").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::put().uri("/user/john").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::delete().uri("/user/john").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post()
            .uri("/user/createWithArrayReal")
            .set_json(serde_json::json!([]))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 501);
    }
}
