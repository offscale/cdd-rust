#![deny(missing_docs)]

//! # Strategies
//!
//! This module defines the architecture for pluggable backend generation.
//!
//! - **traits**: Defines `BackendStrategy` for implementing new frameworks.
//! - **actix**: The default implementation for Actix Web.

pub mod actix;
pub mod traits;

// Re-export for easier access downstream
pub use actix::ActixStrategy;
pub use traits::BackendStrategy;
