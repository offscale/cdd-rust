#![deny(missing_docs)]

//! CDD CLI Programmatic SDK.
//!
//! Exposes programmatic entrypoints for code generation, OpenAPI extraction,
//! schema synchronization, documentation generation, and MCP/JSON-RPC servers.

/// OpenAPI-to-Rust SDK, CLI, and server scaffolding generation.
pub mod from_openapi;
/// Database-to-Diesel model mapper definitions.
pub mod generator;
/// Model Context Protocol (MCP) STDIO server implementation.
pub mod mcp;
/// Route and handler scaffolding commands.
pub mod scaffold;
/// Rust-to-JSON-Schema reflection commands.
pub mod schema_gen;
/// JSON-RPC HTTP server exposing CLI capabilities.
#[cfg(all(feature = "server", not(target_os = "wasi")))]
pub mod serve_json_rpc;
/// Database, OpenAPI, and Rust model synchronization pipeline.
pub mod sync;
/// Contract test suite generator command.
pub mod test_gen;
/// OpenAPI documentation code-snippet JSON generator.
pub mod to_docs_json;
/// Rust-to-OpenAPI specification extractor.
pub mod to_openapi;

pub use from_openapi::{from_openapi, FromOpenApiConfig, FromOpenApiSubcommand, ServerFramework};
#[cfg(all(feature = "server", not(target_os = "wasi")))]
pub use serve_json_rpc::{serve_json_rpc, ServeJsonRpcConfig};
pub use sync::{sync, SyncConfig};
pub use to_docs_json::{to_docs_json, ToDocsJsonConfig};
pub use to_openapi::{to_openapi, ToOpenApiConfig};

/// Global Target Mode Configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum TargetMode {
    /// Actix Web Server.
    #[default]
    ServerActix,
    /// Axum Server.
    ServerAxum,
    /// Reqwest Client.
    ClientReqwest,
    /// Clap CLI / Internal Client.
    ClientInternal,
}
