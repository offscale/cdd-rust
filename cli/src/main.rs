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

use cdd_core::strategies::{ActixStrategy, ClapCliStrategy, ReqwestStrategy};
use cdd_core::AppResult;
use clap::{Parser, Subcommand, ValueEnum};

use crate::generator::DieselMapper;

mod error;
mod from_openapi;
mod generator;
mod scaffold;
mod schema_gen;
#[cfg(feature = "server")]
mod server_json_rpc;
mod sync;
mod test_gen;
mod to_docs_json;
mod to_openapi;

/// Target generation mode.
#[derive(ValueEnum, Clone, Debug, PartialEq)]
pub enum TargetMode {
    /// Generate Actix Web server scaffolding
    Server,
    /// Generate Reqwest client scaffolding
    Client,
    /// Generate Clap CLI scaffolding
    Cli,
}

#[derive(Parser, Debug)]
#[clap(author, version, about = "CDD Toolchain CLI")]
struct Cli {
    #[clap(subcommand)]
    command: Commands,

    /// Target mode (server or client).
    #[clap(
        short,
        long,
        env = "CDD_TARGET",
        default_value = "server",
        global = true
    )]
    target: TargetMode,
}

#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Synchronize DB schema to Rust models and OpenAPI-ready structs.
    Sync(sync::SyncArgs),
    /// Generates integration tests based on OpenAPI contracts.
    TestGen(test_gen::TestGenArgs),
    /// Scaffolds handler functions from OpenAPI Routes.
    Scaffold(scaffold::ScaffoldArgs),
    /// Generates a JSON Schema from a Rust struct or enum.
    #[clap(name = "schema-gen")]
    SchemaGen(schema_gen::SchemaGenArgs),
    /// Generates a JSON output with documentation code snippets for an OpenAPI spec.
    #[clap(name = "to_docs_json")]
    ToDocsJson(to_docs_json::ToDocsJsonArgs),
    /// Generates code from an OpenAPI specification.
    #[clap(name = "from_openapi")]
    FromOpenApi(from_openapi::FromOpenApiArgs),
    /// Generates an OpenAPI specification from source code.
    #[clap(name = "to_openapi")]
    ToOpenApi(to_openapi::ToOpenApiArgs),
    /// Expose CLI interface as JSON-RPC server over HTTP.
    #[clap(name = "serve_json_rpc")]
        #[cfg(feature = "server")]
    ServerJsonRpc(server_json_rpc::ServerJsonRpcArgs),
    #[cfg(not(feature = "server"))]
    ServerJsonRpc,
}

fn main() -> AppResult<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Sync(args) => {
            let mapper = DieselMapper;
            sync::execute(args, &mapper)?;
        }
        Commands::TestGen(args) => match cli.target {
            TargetMode::Server => test_gen::execute(args, &ActixStrategy)?,
            TargetMode::Client => test_gen::execute(args, &ReqwestStrategy)?,
            TargetMode::Cli => test_gen::execute(args, &ClapCliStrategy)?,
        },
        Commands::Scaffold(args) => match cli.target {
            TargetMode::Server => scaffold::execute(args, &ActixStrategy)?,
            TargetMode::Client => scaffold::execute(args, &ReqwestStrategy)?,
            TargetMode::Cli => scaffold::execute(args, &ClapCliStrategy)?,
        },
        Commands::SchemaGen(args) => {
            schema_gen::execute(args)?;
        }
        Commands::ToDocsJson(args) => {
            to_docs_json::execute(args)?;
        }
        Commands::FromOpenApi(args) => {
            from_openapi::execute(args)?;
        }
        Commands::ToOpenApi(args) => {
            to_openapi::execute(args, &cli.target)?;
        }
                #[cfg(feature = "server")]
        Commands::ServerJsonRpc(args) => {
            server_json_rpc::execute(args)?;
        }
        #[cfg(not(feature = "server"))]
        Commands::ServerJsonRpc => {
            return Err(cdd_core::error::AppError::General("Server feature is not compiled".to_string()));
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
