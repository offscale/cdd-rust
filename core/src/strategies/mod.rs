#![deny(missing_docs)]

//! # Strategies
//!
//! This module defines the architecture for pluggable backend generation.
//!
//! - **traits**: Defines `BackendStrategy` for implementing new frameworks.
//! - **actix**: The default implementation for Actix Web.

pub mod actix;
pub mod reqwest;
pub mod traits;

// Re-export for easier access downstream
pub use actix::ActixStrategy;
pub use reqwest::ReqwestStrategy;
pub use traits::BackendStrategy;
pub mod clap_cli;
pub use clap_cli::ClapCliStrategy;
