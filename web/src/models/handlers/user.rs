use actix_web::{web, HttpResponse, Responder};
use actix_multipart::Multipart;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
use chrono::{DateTime, Utc, NaiveDate, NaiveDateTime};

/// Query parameters for `create_user`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateUserQuery {
    /// Created user object
    pub body: User,
}

/// Create user
///
/// This can only be done by the logged in user.
pub async fn create_user(query: web::Query<CreateUserQuery>) -> impl Responder {
    todo!()
}

/// Query parameters for `create_users_with_array_input`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateUsersWithArrayInputQuery {
    /// List of user object
    pub body: Vec<User>,
}

/// Creates list of users with given input array
///
pub async fn create_users_with_array_input(query: web::Query<CreateUsersWithArrayInputQuery>) -> impl Responder {
    todo!()
}

/// Query parameters for `create_users_with_list_input`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateUsersWithListInputQuery {
    /// List of user object
    pub body: Vec<User>,
}

/// Creates list of users with given input array
///
pub async fn create_users_with_list_input(query: web::Query<CreateUsersWithListInputQuery>) -> impl Responder {
    todo!()
}

/// Query parameters for `login_user`.
#[derive(Debug, Clone, Deserialize)]
pub struct LoginUserQuery {
    /// The password for login in clear text
    pub password: String,
    /// The user name for login
    pub username: String,
}

/// Logs user into the system
///
pub async fn login_user(query: web::Query<LoginUserQuery>) -> actix_web::Result<HttpResponse> {
    // Required Response Headers:
    // - X-Expires-After: String (date in UTC when token expires)
    // - X-Rate-Limit: String (calls per hour allowed by the user)
    // Example:
    // HttpResponse::[Status]()
    //     .finish()
    todo!()
}

/// Logs out current logged in user session
///
pub async fn logout_user() -> impl Responder {
    todo!()
}

/// Get user by user name
///
pub async fn get_user_by_name(username: web::Path<String>) -> impl Responder {
    todo!()
}

/// Query parameters for `update_user`.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateUserQuery {
    /// Updated user object
    pub body: User,
}

/// Updated user
///
/// This can only be done by the logged in user.
pub async fn update_user(username: web::Path<String>, query: web::Query<UpdateUserQuery>) -> impl Responder {
    todo!()
}

/// Delete user
///
/// This can only be done by the logged in user.
pub async fn delete_user(username: web::Path<String>) -> impl Responder {
    todo!()
}

