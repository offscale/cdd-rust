//! CDD CLI Programmatic SDK
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

pub mod from_openapi;
pub mod generator;
pub mod scaffold;
pub mod schema_gen;
#[cfg(all(feature = "server", not(target_os = "wasi")))]
pub mod serve_json_rpc;
pub mod sync;
pub mod test_gen;
pub mod to_docs_json;
pub mod to_openapi;

pub use from_openapi::{generate_from_openapi, FromOpenApiConfig, ServerFramework};
#[cfg(all(feature = "server", not(target_os = "wasi")))]
pub use serve_json_rpc::{serve_json_rpc, ServeJsonRpcConfig};
pub use to_docs_json::{generate_docs_json, ToDocsJsonConfig};
pub use to_openapi::{generate_to_openapi, ToOpenApiConfig};

/// Global Target Mode Configuration
#[derive(Clone, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum TargetMode {
    /// Actix Web Server
    #[default]
    ServerActix,
    /// Axum Server
    ServerAxum,
    /// Reqwest Client
    ClientReqwest,
    /// Internal
    ClientInternal,
}
