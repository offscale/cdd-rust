#![warn(missing_docs)]
#![deny(missing_docs)]

//! # CDD CLI
//!
//! Command Line Interface for the Compiler Driven Development toolchain.
//!
//! Supported Commands:
//! - `sync`: Pipeline DB -> Diesel -> Model -> Schema -> OpenAPI.
//! - `from_openapi`: Generate code from an OpenAPI specification.
//! - `to_openapi`: Generate an OpenAPI specification from source code.
//! - `to_docs_json`: Generate JSON documentation.
//! - `serve_json_rpc`: Expose CLI interface as a JSON-RPC server.
//! - `mcp`: Run the generator as an MCP server.

use cdd_cli::generator::DieselMapper;
use cdd_cli::TargetMode;
use cdd_core::AppResult;
use clap::{Parser, Subcommand};

/// The main CLI struct holding all arguments.
#[derive(Parser, Debug)]
#[clap(
    author,
    version,
    about = "CDD Toolchain CLI

Standard Commands:
  from_openapi    Generate code from an OpenAPI specification.
  to_openapi      Generate an OpenAPI specification from source code.
  sync            Synchronize an OpenAPI specification with source code.
  to_docs_json    Generate JSON documentation with code snippets.
  serve_json_rpc  Expose CLI interface as a JSON-RPC server.
  mcp             Run the generator as an MCP server over stdio."
)]
pub struct Cli {
    /// The subcommand to execute.
    #[clap(subcommand)]
    pub command: Commands,

    /// Target mode (server or client).
    #[clap(
        short,
        long,
        env = "CDD_TARGET",
        default_value = "server-actix",
        global = true
    )]
    pub target: TargetMode,
}

/// All available commands in the CLI.
#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Commands {
    /// Synchronize an OpenAPI specification with source code.
    Sync(cdd_cli::sync::SyncArgs),
    /// Generate JSON documentation with code snippets for an OpenAPI specification.
    #[clap(name = "to_docs_json")]
    ToDocsJson(cdd_cli::to_docs_json::ToDocsJsonArgs),
    /// Generate code from an OpenAPI specification.
    #[clap(name = "from_openapi")]
    FromOpenApi(cdd_cli::from_openapi::FromOpenApiArgs),
    /// Generate an OpenAPI specification from source code.
    #[clap(name = "to_openapi")]
    ToOpenApi(cdd_cli::to_openapi::ToOpenApiArgs),
    /// Generate code (SDK, CLI, or Server) from an Arazzo 1.1.0 workflow specification.
    #[clap(name = "from_arazzo")]
    FromArazzo(cdd_cli::from_arazzo::FromArazzoArgs),
    /// Extract an Arazzo 1.1.0 workflow specification from Rust source code.
    #[clap(name = "to_arazzo")]
    ToArazzo(cdd_cli::to_arazzo::ToArazzoArgs),
    /// Expose CLI interface as a JSON-RPC server.
    #[clap(name = "serve_json_rpc")]
    #[cfg(all(feature = "server", not(target_os = "wasi")))]
    ServeJsonRpc(cdd_cli::serve_json_rpc::ServeJsonRpcArgs),
    /// Fallback for missing server feature.
    #[cfg(any(not(feature = "server"), target_os = "wasi"))]
    ServeJsonRpc,
    /// Run the generator as an MCP server over stdio.
    #[clap(name = "mcp")]
    Mcp(cdd_cli::mcp::McpArgs),
}

/// Dispatches a parsed [`Cli`] command to its underlying implementation.
///
/// # Arguments
/// * `cli` - Parsed CLI arguments.
///
/// # Returns
/// * `AppResult<()>` indicating execution success or error.
pub fn dispatch_cli(cli: &Cli) -> AppResult<()> {
    match &cli.command {
        Commands::Sync(args) => {
            let mapper = DieselMapper;
            cdd_cli::sync::run_sync(args, &mapper)
        }
        Commands::ToDocsJson(args) => cdd_cli::to_docs_json::run_to_docs_json(args),
        Commands::FromOpenApi(args) => cdd_cli::from_openapi::run_from_openapi(args),
        Commands::ToOpenApi(args) => cdd_cli::to_openapi::run_to_openapi(args, &cli.target),
        Commands::FromArazzo(args) => cdd_cli::from_arazzo::run_from_arazzo(args),
        Commands::ToArazzo(args) => cdd_cli::to_arazzo::run_to_arazzo(args),
        #[cfg(all(feature = "server", not(target_os = "wasi")))]
        Commands::ServeJsonRpc(args) => cdd_cli::serve_json_rpc::run_serve_json_rpc(args),
        #[cfg(any(not(feature = "server"), target_os = "wasi"))]
        Commands::ServeJsonRpc => Err(cdd_core::error::AppError::General(
            "Server feature is not compiled or not supported on this platform".to_string(),
        )),
        Commands::Mcp(args) => cdd_cli::mcp::run_mcp_server(args),
    }
}

/// The main entry point of the CLI application.
#[cfg(not(test))]
fn main() -> AppResult<()> {
    let cli = Cli::parse();
    dispatch_cli(&cli)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_cli_structure_and_dispatch() {
        use clap::CommandFactory;
        Cli::command().debug_assert();

        let sync_cli = Cli::parse_from(["cdd-rust", "sync", "--truth", "code"]);
        assert!(dispatch_cli(&sync_cli).is_ok());

        let to_openapi_err =
            Cli::parse_from(["cdd-rust", "to_openapi", "-i", "/nonexistent_input_path"]);
        assert!(dispatch_cli(&to_openapi_err).is_err());

        let to_docs_err =
            Cli::parse_from(["cdd-rust", "to_docs_json", "-i", "/nonexistent_spec.yaml"]);
        assert!(dispatch_cli(&to_docs_err).is_err());

        let from_oa_err = Cli::parse_from([
            "cdd-rust",
            "from_openapi",
            "to_sdk",
            "-i",
            "/nonexistent_spec.yaml",
        ]);
        assert!(dispatch_cli(&from_oa_err).is_err());

        let from_arazzo_err = Cli::parse_from([
            "cdd-rust",
            "from_arazzo",
            "to_sdk",
            "-i",
            "/nonexistent_arazzo.yaml",
        ]);
        assert!(dispatch_cli(&from_arazzo_err).is_err());

        let to_arazzo_err =
            Cli::parse_from(["cdd-rust", "to_arazzo", "-i", "/nonexistent_input_path"]);
        assert!(dispatch_cli(&to_arazzo_err).is_err());

        let rpc_err = Cli::parse_from([
            "cdd-rust",
            "serve_json_rpc",
            "-l",
            "invalid.host.address.local",
            "-p",
            "80",
        ]);
        assert!(dispatch_cli(&rpc_err).is_err());

        std::env::set_var("CDD_MCP_TEST_EOF", "1");
        let rpc_ok = Cli::parse_from(["cdd-rust", "serve_json_rpc", "-l", "127.0.0.1", "-p", "0"]);
        assert!(dispatch_cli(&rpc_ok).is_ok());

        let mcp_cli = Cli::parse_from(["cdd-rust", "mcp"]);
        assert!(dispatch_cli(&mcp_cli).is_ok());
    }
}
