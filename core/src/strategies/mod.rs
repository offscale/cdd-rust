#![deny(missing_docs)]

//! # Strategies
//!
//! This module defines the architecture for pluggable backend generation:
//! - [`CodegenStrategy`]: Shared target metadata and module generation hooks.
//! - [`HttpServerStrategy`]: HTTP server extractors, route registration, and test harnesses.
//! - [`HttpClientStrategy`]: HTTP client method generation and mock-server client tests.
//! - [`CliStrategy`]: Clap CLI subcommand dispatch and CLI tests.
//! - [`McpStrategy`]: MCP JSON-RPC / SSE tool registration and dispatch.
//! - [`BackendStrategy`]: Unified adapter interface.

pub mod actix;
pub mod axum;
pub mod clap_cli;
pub mod mcp_client;
pub mod mcp_server;
pub mod reqwest;
pub mod traits;

pub use actix::ActixStrategy;
pub use axum::AxumStrategy;
pub use clap_cli::ClapCliStrategy;
pub use mcp_client::McpClientStrategy;
pub use mcp_server::McpServerStrategy;
pub use reqwest::ReqwestStrategy;
pub use traits::{
    BackendStrategy, CliStrategy, CodegenStrategy, HttpClientStrategy, HttpServerStrategy,
    McpStrategy, StrategyKind,
};
