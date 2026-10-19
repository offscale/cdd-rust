#![deny(missing_docs)]

//! Centralized error handling for the CDD Server.

use cdd_core::error::AppError;
use derive_more::{Display, Error, From};

/// Thin HTTP-aware newtype wrapper around [`AppError`] implementing [`actix_web::ResponseError`].
#[derive(Debug, Display, Error, From)]
#[display("{_0}")]
pub struct ServerError(pub AppError);

impl ServerError {
    /// Constructs a [`ServerError`] representing an unimplemented operation (`501 Not Implemented`).
    ///
    /// # Returns
    /// * `ServerError` wrapping [`AppError::NotImplemented`].
    pub const fn not_implemented() -> Self {
        Self(AppError::NotImplemented)
    }

    /// Constructs a [`ServerError`] representing a configuration failure (`500 Internal Server Error`).
    ///
    /// # Arguments
    /// * `msg` - Error description.
    ///
    /// # Returns
    /// * `ServerError` wrapping [`AppError::ConfigError`].
    pub fn config_error(msg: impl Into<String>) -> Self {
        Self(AppError::ConfigError(msg.into()))
    }

    /// Constructs a [`ServerError`] representing a missing resource (`404 Not Found`).
    ///
    /// # Arguments
    /// * `msg` - Resource description.
    ///
    /// # Returns
    /// * `ServerError` wrapping [`AppError::NotFound`].
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self(AppError::NotFound(msg.into()))
    }

    /// Constructs a [`ServerError`] representing a synchronization failure (`500 Internal Server Error`).
    ///
    /// # Arguments
    /// * `msg` - Lock failure description.
    ///
    /// # Returns
    /// * `ServerError` wrapping [`AppError::SyncError`].
    pub fn sync_error(msg: impl Into<String>) -> Self {
        Self(AppError::SyncError(msg.into()))
    }

    /// Constructs a [`ServerError`] representing an authentication or authorization failure (`401 Unauthorized`).
    ///
    /// # Arguments
    /// * `msg` - Reason for denial.
    ///
    /// # Returns
    /// * `ServerError` wrapping [`AppError::Unauthorized`].
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self(AppError::Unauthorized(msg.into()))
    }

    /// Constructs a [`ServerError`] representing a request validation failure (`400 Bad Request`).
    ///
    /// # Arguments
    /// * `msg` - Validation failure reason.
    ///
    /// # Returns
    /// * `ServerError` wrapping [`AppError::ValidationError`].
    pub fn validation_error(msg: impl Into<String>) -> Self {
        Self(AppError::ValidationError(msg.into()))
    }

    /// Returns a reference to the inner [`AppError`].
    ///
    /// # Returns
    /// * `&AppError` wrapped by this instance.
    pub const fn inner(&self) -> &AppError {
        &self.0
    }
}

impl From<diesel::result::Error> for ServerError {
    /// Converts a Diesel database error into a [`ServerError`].
    ///
    /// # Arguments
    /// * `err` - The Diesel error encountered.
    ///
    /// # Returns
    /// * `ServerError` wrapping [`AppError::NotFound`] or [`AppError::Database`].
    fn from(err: diesel::result::Error) -> Self {
        match err {
            diesel::result::Error::NotFound => Self(AppError::NotFound(err.to_string())),
            other => Self(AppError::Database(other.to_string())),
        }
    }
}

impl From<uuid::Error> for ServerError {
    /// Converts a UUID parsing error into a [`ServerError`].
    ///
    /// # Arguments
    /// * `err` - The UUID parsing error.
    ///
    /// # Returns
    /// * `ServerError` wrapping [`AppError::ValidationError`].
    fn from(err: uuid::Error) -> Self {
        Self(AppError::ValidationError(format!("UUID Error: {err}")))
    }
}

/// Convert `ServerError` to `actix_web::ResponseError` on non-WASI targets.
#[cfg(not(target_os = "wasi"))]
impl actix_web::ResponseError for ServerError {
    /// Maps the wrapped [`AppError`] variant to its corresponding HTTP status code.
    ///
    /// # Returns
    /// * `actix_web::http::StatusCode` representing the error category.
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self.0 {
            AppError::NotFound(_) => actix_web::http::StatusCode::NOT_FOUND,
            AppError::NotImplemented => actix_web::http::StatusCode::NOT_IMPLEMENTED,
            AppError::Unauthorized(_) => actix_web::http::StatusCode::UNAUTHORIZED,
            AppError::ValidationError(_) | AppError::MissingArgument { .. } => {
                actix_web::http::StatusCode::BAD_REQUEST
            }
            _ => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes_and_constructors() {
        use actix_web::ResponseError;
        let core_err = ServerError::from(AppError::General("core".to_string()));
        assert!(matches!(core_err.inner(), AppError::General(_)));
        assert_eq!(
            core_err.status_code(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ServerError::not_implemented().status_code(),
            actix_web::http::StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            ServerError::config_error("err").status_code(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ServerError::sync_error("err").status_code(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ServerError::not_found("missing").status_code(),
            actix_web::http::StatusCode::NOT_FOUND
        );
        assert_eq!(
            ServerError::from(diesel::result::Error::NotFound).status_code(),
            actix_web::http::StatusCode::NOT_FOUND
        );
        assert_eq!(
            ServerError::from(diesel::result::Error::RollbackTransaction).status_code(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ServerError::unauthorized("bad token").status_code(),
            actix_web::http::StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            ServerError::validation_error("invalid field").status_code(),
            actix_web::http::StatusCode::BAD_REQUEST
        );
        let uuid_err = uuid::Uuid::parse_str("invalid").expect_err("must fail");
        assert_eq!(
            ServerError::from(uuid_err).status_code(),
            actix_web::http::StatusCode::BAD_REQUEST
        );
    }
}
