#![deny(missing_docs)]

//! # Strategies
//!
//! This module defines the architecture for pluggable backend generation.
//!
//! - **traits**: Defines `BackendStrategy` for implementing new frameworks.
//! - **actix**: The default implementation for Actix Web.

pub mod actix;
pub mod axum;
pub mod reqwest;
pub mod traits;

// Re-export for easier access downstream
pub use actix::ActixStrategy;
pub use axum::AxumStrategy;
pub use reqwest::ReqwestStrategy;
pub use traits::BackendStrategy;
pub mod clap_cli;
pub mod mcp_client;
pub mod mcp_server;
pub use clap_cli::ClapCliStrategy;
pub use mcp_client::McpClientStrategy;
pub use mcp_server::McpServerStrategy;
