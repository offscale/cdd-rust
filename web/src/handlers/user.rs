#![deny(missing_docs)]

//! User management and authentication HTTP handlers.

use crate::dao::factory::AppDaos;
use crate::error::ServerError;
use crate::models::handlers::user::LoginUserQuery;
use crate::models::users::{CreateUsers, UpdateUsers};
use crate::models::User;
use crate::security::{create_jwt, hash_password, verify_password};
use actix_web::{web, HttpResponse, Responder};
use uuid::Uuid;

/// Creates a new user (`POST /user`).
///
/// # Arguments
/// * `daos` - Optional shared [`AppDaos`] container.
/// * `body` - Optional strongly-typed [`User`] payload.
///
/// # Returns
/// * `200 OK` with created user metadata.
///
/// # Errors
/// * Returns [`ServerError`] if persistence fails.
pub async fn create_user(
    daos: Option<web::Data<AppDaos>>,
    body: Option<web::Json<User>>,
) -> Result<HttpResponse, ServerError> {
    let payload = body.map(|b| b.into_inner()).unwrap_or_default();
    let email = payload.resolved_identity();
    let password = payload.password.as_deref().unwrap_or("secret");
    if let Some(daos) = daos {
        let now = chrono::Utc::now().naive_utc();
        daos.user_dao
            .create_user(CreateUsers {
                id: Uuid::new_v4(),
                email: email.clone(),
                password_hash: hash_password(password),
                created_at: now,
                updated_at: now,
            })
            .await?;
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({ "email": email })))
}

/// Creates a list of users from the given input list (`POST /user/createWithList`).
///
/// # Arguments
/// * `daos` - Optional shared [`AppDaos`] container.
/// * `body` - Optional JSON array of [`User`] objects.
///
/// # Returns
/// * `200 OK` response with count of processed users.
///
/// # Errors
/// * Returns [`ServerError`] if persistence fails.
pub async fn create_users_with_list_input(
    daos: Option<web::Data<AppDaos>>,
    body: Option<web::Json<Vec<User>>>,
) -> Result<HttpResponse, ServerError> {
    let items = body.map(|b| b.into_inner()).unwrap_or_default();
    if let Some(daos) = daos {
        let now = chrono::Utc::now().naive_utc();
        for item in &items {
            let email = item.resolved_identity();
            let pw = item.password.as_deref().unwrap_or("secret");
            daos.user_dao
                .create_user(CreateUsers {
                    id: Uuid::new_v4(),
                    email,
                    password_hash: hash_password(pw),
                    created_at: now,
                    updated_at: now,
                })
                .await?;
        }
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({ "created": items.len() })))
}

/// Authenticates a user into the system (`GET /user/login`).
///
/// # Arguments
/// * `query` - Optional [`LoginUserQuery`] credentials.
/// * `daos` - Optional shared [`AppDaos`] container for credential verification.
///
/// # Returns
/// * `200 OK` response containing a signed bearer token, or `401 Unauthorized` if password verification fails.
///
/// # Errors
/// * Returns [`ServerError`] if token creation or persistence lookup fails.
pub async fn login_user(
    query: Option<web::Query<LoginUserQuery>>,
    daos: Option<web::Data<AppDaos>>,
) -> Result<HttpResponse, ServerError> {
    let (username, password) = query
        .map(|q| {
            let inner = q.into_inner();
            (inner.username, inner.password)
        })
        .unwrap_or_else(|| ("anonymous".to_string(), "secret".to_string()));

    if let Some(daos) = daos {
        if let Some(existing) = daos.user_dao.get_user_by_name(&username).await? {
            if !verify_password(&password, &existing.password_hash) {
                return Err(ServerError::unauthorized("Invalid username or password"));
            }
        }
    }

    let token = create_jwt(&username)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "token": token })))
}

/// Logs out the current active user session (`GET /user/logout`).
///
/// # Returns
/// * `200 OK` response.
pub async fn logout_user() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({ "status": "logged_out" }))
}

/// Retrieves a user by username (`GET /user/{username}`).
///
/// # Arguments
/// * `username` - Path parameter identifying the user.
/// * `daos` - Optional shared [`AppDaos`] container.
///
/// # Returns
/// * `200 OK` with user JSON if found (or in stateless mode), or `404 Not Found`.
///
/// # Errors
/// * Returns [`ServerError`] on DAO error.
pub async fn get_user_by_name(
    username: web::Path<String>,
    daos: Option<web::Data<AppDaos>>,
) -> Result<HttpResponse, ServerError> {
    let name = username.into_inner();
    if let Some(daos) = daos {
        if let Some(user) = daos.user_dao.get_user_by_name(&name).await? {
            return Ok(HttpResponse::Ok().json(user));
        }
        return Ok(HttpResponse::NotFound().finish());
    }
    Ok(HttpResponse::Ok().json(serde_json::json!({ "username": name })))
}

/// Updates an existing user record (`PUT /user/{username}`).
///
/// # Arguments
/// * `username` - Path parameter identifying the user.
/// * `daos` - Optional shared [`AppDaos`] container.
/// * `body` - Optional strongly-typed [`User`] update payload.
///
/// # Returns
/// * `200 OK` response.
///
/// # Errors
/// * Returns [`ServerError`] on DAO error.
pub async fn update_user(
    username: web::Path<String>,
    daos: Option<web::Data<AppDaos>>,
    body: Option<web::Json<User>>,
) -> Result<HttpResponse, ServerError> {
    let name = username.into_inner();
    if let Some(daos) = daos {
        if let Some(existing) = daos.user_dao.get_user_by_name(&name).await? {
            let new_email = body
                .and_then(|b| b.into_inner().email)
                .or(Some(name.clone()));
            daos.user_dao
                .update_user(
                    existing.id,
                    UpdateUsers {
                        email: new_email,
                        password_hash: None,
                        created_at: None,
                        updated_at: Some(chrono::Utc::now().naive_utc()),
                    },
                )
                .await?;
        }
    }
    Ok(HttpResponse::Ok().finish())
}

/// Deletes a user record (`DELETE /user/{username}`).
///
/// # Arguments
/// * `username` - Path parameter identifying the user.
/// * `daos` - Optional shared [`AppDaos`] container.
///
/// # Returns
/// * `200 OK` response.
///
/// # Errors
/// * Returns [`ServerError`] on DAO error.
pub async fn delete_user(
    username: web::Path<String>,
    daos: Option<web::Data<AppDaos>>,
) -> Result<HttpResponse, ServerError> {
    let name = username.into_inner();
    if let Some(daos) = daos {
        if let Some(existing) = daos.user_dao.get_user_by_name(&name).await? {
            daos.user_dao.delete_user(existing.id).await?;
        }
    }
    Ok(HttpResponse::Ok().finish())
}

/// Creates a list of users from an input array (`POST /user/createWithArray`).
///
/// # Arguments
/// * `daos` - Optional shared [`AppDaos`] container.
/// * `body` - Optional JSON array of [`crate::models::User`] objects.
///
/// # Returns
/// * `200 OK` response with count of processed users.
///
/// # Errors
/// * Returns [`ServerError`] if persistence fails.
pub async fn create_users_with_array_input(
    daos: Option<web::Data<AppDaos>>,
    body: Option<web::Json<Vec<User>>>,
) -> Result<HttpResponse, ServerError> {
    create_users_with_list_input(daos, body).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dao::users::{InMemoryUserDao, StubUserDao};
    use actix_web::{test, App};
    use serde_json::Value;
    use std::sync::Arc;

    #[actix_web::test]
    async fn test_user_handlers_stateless_and_dao_backed() {
        let app = test::init_service(
            App::new()
                .route("/user", web::post().to(create_user))
                .route(
                    "/user/createWithList",
                    web::post().to(create_users_with_list_input),
                )
                .route(
                    "/user/createWithArray",
                    web::post().to(create_users_with_array_input),
                )
                .route("/user/login", web::get().to(login_user))
                .route("/user/logout", web::get().to(logout_user))
                .route("/user/{username}", web::get().to(get_user_by_name))
                .route("/user/{username}", web::put().to(update_user))
                .route("/user/{username}", web::delete().to(delete_user)),
        )
        .await;

        let req = test::TestRequest::post().uri("/user").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post()
            .uri("/user/createWithList")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post()
            .uri("/user/createWithArray")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post()
            .uri("/user/createWithArray")
            .set_json(serde_json::json!([{"username": "arr_user"}]))
            .to_request();
        let arr_body: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(arr_body["created"], 1);

        let req = test::TestRequest::get()
            .uri("/user/login?username=john&password=pw")
            .to_request();
        let login_body: Value = test::call_and_read_body_json(&app, req).await;
        assert!(login_body["token"]
            .as_str()
            .expect("token")
            .starts_with("cdd."));

        let req = test::TestRequest::get().uri("/user/login").to_request();
        assert!(test::call_service(&app, req).await.status().is_success());

        let req = test::TestRequest::get().uri("/user/logout").to_request();
        assert!(test::call_service(&app, req).await.status().is_success());

        let req = test::TestRequest::get().uri("/user/john").to_request();
        assert!(test::call_service(&app, req).await.status().is_success());

        let req = test::TestRequest::put().uri("/user/john").to_request();
        assert!(test::call_service(&app, req).await.status().is_success());

        let req = test::TestRequest::delete().uri("/user/john").to_request();
        assert!(test::call_service(&app, req).await.status().is_success());

        // DAO-backed lifecycle test
        let daos = web::Data::new(AppDaos {
            user_dao: Arc::new(InMemoryUserDao::new()),
        });
        let dao_app = test::init_service(
            App::new()
                .app_data(daos)
                .route("/user", web::post().to(create_user))
                .route(
                    "/user/createWithList",
                    web::post().to(create_users_with_list_input),
                )
                .route("/user/login", web::get().to(login_user))
                .route("/user/{username}", web::get().to(get_user_by_name))
                .route("/user/{username}", web::put().to(update_user))
                .route("/user/{username}", web::delete().to(delete_user)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/user")
            .set_json(serde_json::json!({"username": "john", "password": "pw"}))
            .to_request();
        let created: Value = test::call_and_read_body_json(&dao_app, req).await;
        assert_eq!(created["email"], "john");

        let req = test::TestRequest::get()
            .uri("/user/login?username=john&password=pw")
            .to_request();
        assert!(test::call_service(&dao_app, req)
            .await
            .status()
            .is_success());

        let req = test::TestRequest::get()
            .uri("/user/login?username=john&password=wrong")
            .to_request();
        assert_eq!(
            test::call_service(&dao_app, req).await.status(),
            actix_web::http::StatusCode::UNAUTHORIZED
        );

        let req = test::TestRequest::get()
            .uri("/user/login?username=unregistered&password=pw")
            .to_request();
        assert!(test::call_service(&dao_app, req)
            .await
            .status()
            .is_success());

        let req = test::TestRequest::post()
            .uri("/user/createWithList")
            .set_json(serde_json::json!([{"email": "jane"}, {}]))
            .to_request();
        let list_res: Value = test::call_and_read_body_json(&dao_app, req).await;
        assert_eq!(list_res["created"], 2);

        let req = test::TestRequest::get().uri("/user/john").to_request();
        let found: Value = test::call_and_read_body_json(&dao_app, req).await;
        assert_eq!(found["email"], "john");

        let req = test::TestRequest::get().uri("/user/missing").to_request();
        let resp = test::call_service(&dao_app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);

        let req = test::TestRequest::put()
            .uri("/user/john")
            .set_json(serde_json::json!({"email": "john_updated"}))
            .to_request();
        assert!(test::call_service(&dao_app, req)
            .await
            .status()
            .is_success());

        let req = test::TestRequest::put().uri("/user/missing").to_request();
        assert!(test::call_service(&dao_app, req)
            .await
            .status()
            .is_success());

        let req = test::TestRequest::delete()
            .uri("/user/john_updated")
            .to_request();
        assert!(test::call_service(&dao_app, req)
            .await
            .status()
            .is_success());

        let req = test::TestRequest::delete()
            .uri("/user/john_updated")
            .to_request();
        assert!(test::call_service(&dao_app, req)
            .await
            .status()
            .is_success());

        // Stub DAO error propagation test
        let stub_daos = web::Data::new(AppDaos {
            user_dao: Arc::new(StubUserDao),
        });
        let err_app = test::init_service(
            App::new()
                .app_data(stub_daos)
                .route("/user", web::post().to(create_user))
                .route(
                    "/user/createWithList",
                    web::post().to(create_users_with_list_input),
                )
                .route("/user/login", web::get().to(login_user))
                .route("/user/{username}", web::get().to(get_user_by_name))
                .route("/user/{username}", web::put().to(update_user))
                .route("/user/{username}", web::delete().to(delete_user)),
        )
        .await;

        assert_eq!(
            test::call_service(
                &err_app,
                test::TestRequest::post().uri("/user").to_request()
            )
            .await
            .status(),
            actix_web::http::StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            test::call_service(
                &err_app,
                test::TestRequest::post()
                    .uri("/user/createWithList")
                    .set_json(serde_json::json!([{"email": "a"}]))
                    .to_request()
            )
            .await
            .status(),
            actix_web::http::StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            test::call_service(
                &err_app,
                test::TestRequest::get().uri("/user/login").to_request()
            )
            .await
            .status(),
            actix_web::http::StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            test::call_service(
                &err_app,
                test::TestRequest::get().uri("/user/john").to_request()
            )
            .await
            .status(),
            actix_web::http::StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            test::call_service(
                &err_app,
                test::TestRequest::put().uri("/user/john").to_request()
            )
            .await
            .status(),
            actix_web::http::StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            test::call_service(
                &err_app,
                test::TestRequest::delete().uri("/user/john").to_request()
            )
            .await
            .status(),
            actix_web::http::StatusCode::NOT_IMPLEMENTED
        );
    }
}
