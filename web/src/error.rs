#![deny(missing_docs)]

//! Centralized error handling for the CDD Server.

use cdd_core::error::AppError;
use derive_more::{Display, Error, From};

/// A consolidated error type for all server operations.
#[derive(Debug, Display, Error, From)]
pub enum ServerError {
    /// Core library error.
    #[display("Core error: {_0}")]
    Core(AppError),

    /// A database constraint violation or connection error.
    #[display("Database error: {_0}")]
    DatabaseError(diesel::result::Error),

    /// Missing configuration or invalid setup.
    #[display("Configuration error: {_0}")]
    #[from(ignore)]
    #[error(ignore)]
    ConfigError(String),

    /// A generic stub error when functionality is not implemented.
    #[display("Not implemented")]
    NotImplemented,

    /// UUID parsing error.
    #[display("UUID Error: {_0}")]
    UuidError(uuid::Error),

    /// Threading / synchronization error.
    #[display("Synchronization error: {_0}")]
    #[from(ignore)]
    #[error(ignore)]
    SyncError(String),

    /// Authentication or authorization failure.
    #[display("Unauthorized: {_0}")]
    #[from(ignore)]
    #[error(ignore)]
    Unauthorized(String),

    /// Request validation failure.
    #[display("Validation error: {_0}")]
    #[from(ignore)]
    #[error(ignore)]
    ValidationError(String),
}

/// Convert `ServerError` to `actix_web::ResponseError` on non-WASI targets.
#[cfg(not(target_os = "wasi"))]
impl actix_web::ResponseError for ServerError {
    /// Maps each [`ServerError`] variant to its corresponding HTTP status code.
    ///
    /// # Returns
    /// * `actix_web::http::StatusCode` representing the error category.
    fn status_code(&self) -> actix_web::http::StatusCode {
        match *self {
            ServerError::Core(_) => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::DatabaseError(diesel::result::Error::NotFound) => {
                actix_web::http::StatusCode::NOT_FOUND
            }
            ServerError::DatabaseError(_) => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::ConfigError(_) => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::NotImplemented => actix_web::http::StatusCode::NOT_IMPLEMENTED,
            ServerError::UuidError(_) => actix_web::http::StatusCode::BAD_REQUEST,
            ServerError::SyncError(_) => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::Unauthorized(_) => actix_web::http::StatusCode::UNAUTHORIZED,
            ServerError::ValidationError(_) => actix_web::http::StatusCode::BAD_REQUEST,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        use actix_web::ResponseError;
        assert_eq!(
            ServerError::Core(AppError::General("core".to_string())).status_code(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ServerError::NotImplemented.status_code(),
            actix_web::http::StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            ServerError::ConfigError("err".to_string()).status_code(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ServerError::SyncError("err".to_string()).status_code(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ServerError::DatabaseError(diesel::result::Error::NotFound).status_code(),
            actix_web::http::StatusCode::NOT_FOUND
        );
        assert_eq!(
            ServerError::DatabaseError(diesel::result::Error::RollbackTransaction).status_code(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ServerError::Unauthorized("bad token".to_string()).status_code(),
            actix_web::http::StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            ServerError::ValidationError("invalid field".to_string()).status_code(),
            actix_web::http::StatusCode::BAD_REQUEST
        );
        let uuid_err = uuid::Uuid::parse_str("invalid").expect_err("must fail");
        assert_eq!(
            ServerError::UuidError(uuid_err).status_code(),
            actix_web::http::StatusCode::BAD_REQUEST
        );
    }
}
