#![warn(missing_docs)]
#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

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

use cdd_core::AppResult;
use clap::{Parser, Subcommand, ValueEnum};

use cdd_cli::generator::DieselMapper;

/// Target generation mode.
#[derive(ValueEnum, Clone, Debug, PartialEq)]
pub enum TargetModeDuplicate {
    /// Generate Actix Web server scaffolding.
    ServerActix,
    /// Generate Axum server scaffolding.
    ServerAxum,
    /// Generate Reqwest client scaffolding.
    Client,
    /// Generate Clap CLI scaffolding.
    Cli,
}

/// The main CLI struct holding all arguments.
#[derive(Parser, Debug)]
#[clap(
    author,
    version,
    about = "CDD Toolchain CLI\n\nStandard Commands:\n  from_openapi    Generate code from an OpenAPI specification.\n  to_openapi      Generate an OpenAPI specification from source code.\n  sync            Synchronize an OpenAPI specification with source code.\n  to_docs_json    Generate JSON documentation with code snippets.\n  serve_json_rpc  Expose CLI interface as a JSON-RPC server.\n  mcp             Run the generator as an MCP server over stdio."
)]
struct Cli {
    /// The subcommand to execute.
    #[clap(subcommand)]
    command: Commands,

    /// Target mode (server or client).
    #[clap(
        short,
        long,
        env = "CDD_TARGET",
        default_value = "server-actix",
        global = true
    )]
    target: cdd_cli::TargetMode,
}

/// All available commands in the CLI.
#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
enum Commands {
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

/// The main entry point of the CLI application.
fn main() -> AppResult<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Sync(args) => {
            let mapper = DieselMapper;
            cdd_cli::sync::run_sync(args, &mapper)?;
        }
        Commands::ToDocsJson(args) => {
            cdd_cli::to_docs_json::run_to_docs_json(args)?;
        }
        Commands::FromOpenApi(args) => {
            cdd_cli::from_openapi::run_from_openapi(args)?;
        }
        Commands::ToOpenApi(args) => {
            cdd_cli::to_openapi::run_to_openapi(args, &cli.target)?;
        }
        #[cfg(all(feature = "server", not(target_os = "wasi")))]
        Commands::ServeJsonRpc(args) => {
            cdd_cli::serve_json_rpc::run_serve_json_rpc(args)?;
        }
        #[cfg(any(not(feature = "server"), target_os = "wasi"))]
        Commands::ServeJsonRpc => {
            return Err(cdd_core::error::AppError::General(
                "Server feature is not compiled or not supported on this platform".to_string(),
            ));
        }
        Commands::Mcp(args) => {
            cdd_cli::mcp::run_mcp_server(args)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_cli_structure() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
