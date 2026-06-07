#![warn(missing_docs)]
#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # CDD CLI
//!
//! Command Line Interface for the Compiler Driven Development toolchain.
//!
//! Supported Commands:
//! - `sync`: Pipeline DB -> Diesel -> Model -> Schema -> OpenAPI.
//! - `test-gen`: Generates integration tests from OpenAPI specs.
//! - `scaffold`: Generates handler scaffolding from OpenAPI specs.
//! - `schema-gen`: Generates JSON Schemas from Rust structs.

use cdd_core::strategies::{ActixStrategy, AxumStrategy, ClapCliStrategy, ReqwestStrategy};
use cdd_core::AppResult;
use clap::{Parser, Subcommand, ValueEnum};

use cdd_cli::generator::DieselMapper;

// mod from_openapi;
// mod generator;
// mod scaffold;
// mod schema_gen;
// mod serve_json_rpc;
// mod sync;
// mod test_gen;
// mod to_docs_json;
// mod to_openapi;

/// Target generation mode.
#[derive(ValueEnum, Clone, Debug, PartialEq)]
pub enum TargetModeDuplicate {
    /// Generate Actix Web server scaffolding
    ServerActix,
    /// Generate Axum server scaffolding
    ServerAxum,
    /// Generate Reqwest client scaffolding
    Client,
    /// Generate Clap CLI scaffolding
    Cli,
}

/// The main CLI struct holding all arguments.
#[derive(Parser, Debug)]
#[clap(
    author,
    version,
    about = "CDD Toolchain CLI\n\nLanguage-Specific Commands:\n  sync        Synchronize DB schema to Rust models and OpenAPI-ready structs.\n  test-gen    Generates integration tests based on OpenAPI contracts.\n  scaffold    Scaffolds handler functions from OpenAPI Routes.\n  schema-gen  Generates a JSON Schema from a Rust struct or enum."
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
    /// Synchronize DB schema to Rust models and OpenAPI-ready structs.
    Sync(cdd_cli::sync::SyncArgs),
    /// Generates integration tests based on OpenAPI contracts.
    TestGen(cdd_cli::test_gen::TestGenArgs),
    /// Scaffolds handler functions from OpenAPI Routes.
    Scaffold(cdd_cli::scaffold::ScaffoldArgs),
    /// Generates a JSON Schema from a Rust struct or enum.
    #[clap(name = "schema-gen")]
    SchemaGen(cdd_cli::schema_gen::SchemaGenArgs),
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
    /// Fallback for missing server feature
    #[cfg(any(not(feature = "server"), target_os = "wasi"))]
    ServeJsonRpc,
    /// Expose CLI interface as an MCP server over STDIO.
    #[clap(name = "mcp")]
    Mcp(cdd_cli::mcp::McpArgs),
}

/// The main entry point of the CLI application.
fn main() -> AppResult<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Sync(args) => {
            let mapper = DieselMapper;
            cdd_cli::sync::execute(args, &mapper)?;
        }
        Commands::TestGen(args) => match cli.target {
            cdd_cli::TargetMode::ServerActix => cdd_cli::test_gen::execute(args, &ActixStrategy)?,
            cdd_cli::TargetMode::ServerAxum => cdd_cli::test_gen::execute(args, &AxumStrategy)?,
            cdd_cli::TargetMode::ClientReqwest => {
                cdd_cli::test_gen::execute(args, &ReqwestStrategy)?
            }
            cdd_cli::TargetMode::ClientInternal => {
                cdd_cli::test_gen::execute(args, &ClapCliStrategy)?
            }
        },
        Commands::Scaffold(args) => match cli.target {
            cdd_cli::TargetMode::ServerActix => cdd_cli::scaffold::execute(args, &ActixStrategy)?,
            cdd_cli::TargetMode::ServerAxum => cdd_cli::scaffold::execute(args, &AxumStrategy)?,
            cdd_cli::TargetMode::ClientReqwest => {
                cdd_cli::scaffold::execute(args, &ReqwestStrategy)?
            }
            cdd_cli::TargetMode::ClientInternal => {
                cdd_cli::scaffold::execute(args, &ClapCliStrategy)?
            }
        },
        Commands::SchemaGen(args) => {
            cdd_cli::schema_gen::execute(args)?;
        }
        Commands::ToDocsJson(args) => {
            cdd_cli::to_docs_json::execute(args)?;
        }
        Commands::FromOpenApi(args) => {
            cdd_cli::from_openapi::execute(args)?;
        }
        Commands::ToOpenApi(args) => {
            cdd_cli::to_openapi::execute(args, &cli.target)?;
        }
        #[cfg(all(feature = "server", not(target_os = "wasi")))]
        Commands::ServeJsonRpc(args) => {
            cdd_cli::serve_json_rpc::execute(args)?;
        }
        #[cfg(any(not(feature = "server"), target_os = "wasi"))]
        Commands::ServeJsonRpc => {
            return Err(cdd_core::error::AppError::General(
                "Server feature is not compiled or not supported on this platform".to_string(),
            ));
        }
        Commands::Mcp(args) => {
            cdd_cli::mcp::serve_mcp(args)?;
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
