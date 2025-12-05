#![deny(missing_docs)]

//! # CLI Errors
//!
//! Error types for the CLI crate.

use derive_more::{Display, From};

/// Main error enum for CLI operations.
#[derive(Debug, Display, From)]
pub enum CliError {
    /// IO Error wrapper.
    #[display("IO Error: {}", _0)]
    Io(std::io::Error),

    /// General failure message.
    #[display("Operation failed: {}", _0)]
    General(String),
}

/// Manual implementation of the standard Error trait.
///
/// We implement this manually (instead of `derive(Error)`) because the `General(String)`
/// variant contains a `String`, which does not implement `std::error::Error`, causing
/// auto-derived `source()` implementations to fail compilation.
impl std::error::Error for CliError {}

/// Result type alias.
pub type CliResult<T> = Result<T, CliError>;
