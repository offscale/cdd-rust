#![deny(missing_docs)]

//! # CLI Errors
//!
//! Error types for the CLI crate.

use derive_more::{Display, Error, From};

/// Main error enum for CLI operations.
#[derive(Debug, Display, From, Error)]
pub enum CliError {
    /// IO Error wrapper.
    #[display(fmt = "IO Error: {}", _0)]
    Io(std::io::Error),

    /// General failure message.
    #[display(fmt = "Operation failed: {}", _0)]
    General(String),
}

/// Result type alias.
pub type CliResult<T> = Result<T, CliError>;
