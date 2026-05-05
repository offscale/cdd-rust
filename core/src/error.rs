//! # Error Handling
//!
//! Provides the unified `AppError` enum used across the workspace.

use derive_more::{Display, From};

/// The Global Error Enum.
///
/// We use `derive_more` for boilerplate.
#[derive(Debug, Display, From)]
pub enum AppError {
    /// Wrapper for standard IO errors.
    #[display("IO Error: {_0}")]
    Io(std::io::Error),

    /// Wrapper for format errors.
    #[display("Format Error: {_0}")]
    Fmt(std::fmt::Error),

    /// Wrapper for JSON errors.
    #[display("JSON Error: {_0}")]
    SerdeJson(serde_json::Error),

    /// Wrapper for YAML errors.
    #[display("YAML Error: {_0}")]
    SerdeYaml(serde_yaml::Error),

    /// Wrapper for URL parse errors.
    #[display("URL Parse Error: {_0}")]
    UrlParse(url::ParseError),

    /// Wrapper for Utf8 errors.
    #[display("UTF-8 Error: {_0}")]
    Utf8(std::str::Utf8Error),

    /// Wrapper for Database string errors.
    /// We ignore this for `From<String>` to avoid conflict with General.
    #[from(ignore)]
    #[display("Database Error: {_0}")]
    Database(String),

    /// Generic errors.
    #[display("General Error: {_0}")]
    General(String),
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::General(s.to_string())
    }
}

/// Manual implementation of the standard Error trait.
impl std::error::Error for AppError {}

/// Helper type alias for Result using AppError.
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod additional_tests {
    use super::*;

    #[test]
    fn test_from_str() {
        let err: AppError = "some error".into();
        assert_eq!(format!("{}", err), "General Error: some error");
    }
}
