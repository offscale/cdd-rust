//! Centralized error handling for the CDD Server.

use derive_more::{Display, Error, From};

/// A consolidated error type for all server operations.
#[derive(Debug, Display, Error, From)]
pub enum ServerError {
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
}

/// Convert ServerError to actix_web ResponseError if not WASM
#[cfg(not(target_os = "wasi"))]
impl actix_web::ResponseError for ServerError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        match *self {
            ServerError::DatabaseError(_) => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::ConfigError(_) => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            ServerError::NotImplemented => actix_web::http::StatusCode::NOT_IMPLEMENTED,
            ServerError::UuidError(_) => actix_web::http::StatusCode::BAD_REQUEST,
            ServerError::SyncError(_) => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
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
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        let uuid_err = uuid::Uuid::parse_str("invalid").unwrap_err();
        assert_eq!(
            ServerError::UuidError(uuid_err).status_code(),
            actix_web::http::StatusCode::BAD_REQUEST
        );
    }
}
