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
pub async fn create_users_with_array_input(body: web::Json<Vec<User>>) -> impl Responder {
    todo!()
}
