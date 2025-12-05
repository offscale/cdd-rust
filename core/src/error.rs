//! # Error Handling
//!
//! Provides the unified `AppError` enum used across the workspace.

use derive_more::{Display, From};

/// The Global Error Enum.
///
/// We use `derive_more` for boilerplate.
/// Note: String errors default to `General`.
#[derive(Debug, Display, From)]
pub enum AppError {
    /// Wrapper for standard IO errors.
    #[display("IO Error: {_0}")]
    Io(std::io::Error),

    /// Wrapper for Database string errors.
    /// We ignore this for `From<String>` to avoid conflict with General.
    #[from(ignore)]
    #[display("Database Error: {_0}")]
    Database(String),

    /// Generic errors.
    #[display("General Error: {_0}")]
    General(String),
}

/// Manual implementation of the standard Error trait.
impl std::error::Error for AppError {}

/// Helper type alias for Result using AppError.
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    #[test]
    fn test_io_conversion() {
        let io_err = Error::new(ErrorKind::Other, "test");
        let app_err: AppError = io_err.into();
        assert!(matches!(app_err, AppError::Io(_)));
    }

    #[test]
    fn test_string_conversion() {
        // Test that String defaults to General, not Database
        let msg = String::from("something wrong");
        let app_err: AppError = msg.into();
        match app_err {
            AppError::General(s) => assert_eq!(s, "something wrong"),
            _ => panic!("String should convert to AppError::General"),
        }
    }

    #[test]
    fn test_database_manual_creation() {
        // Database errors must be created explicitly
        let app_err = AppError::Database("db fail".into());
        assert_eq!(format!("{}", app_err), "Database Error: db fail");
    }
}
