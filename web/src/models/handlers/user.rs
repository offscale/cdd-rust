#![deny(missing_docs)]

//! User endpoint parameter models.

use serde::{Deserialize, Serialize};

/// Query parameters for `GET /user/login`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LoginUserQuery {
    /// The user name for login.
    pub username: String,
    /// The password for login in clear text.
    pub password: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App, HttpResponse};

    #[actix_web::test]
    async fn test_login_user_query_extractor() {
        let app = test::init_service(App::new().route(
            "/user/login",
            web::get().to(|q: web::Query<LoginUserQuery>| async move {
                HttpResponse::Ok().json(q.into_inner())
            }),
        ))
        .await;

        let req = test::TestRequest::get()
            .uri("/user/login?username=alice&password=secret")
            .to_request();
        let extracted: LoginUserQuery = test::call_and_read_body_json(&app, req).await;
        assert_eq!(
            extracted,
            LoginUserQuery {
                username: "alice".to_string(),
                password: "secret".to_string(),
            }
        );
    }
}
