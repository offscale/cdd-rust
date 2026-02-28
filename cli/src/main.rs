#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # CDD CLI
//!
//! Command Line Interface for the Contract-Driven Development toolchain.
//!
//! Supported Commands:
//! - `sync`: Pipeline DB -> Diesel -> Model -> Schema -> OpenAPI.
//! - `test-gen`: Generates integration tests from OpenAPI specs.
//! - `scaffold`: Generates handler scaffolding from OpenAPI specs.
//! - `schema-gen`: Generates JSON Schemas from Rust structs.

use cdd_core::strategies::ActixStrategy;
use cdd_core::AppResult;
use clap::{Parser, Subcommand};

use crate::generator::DieselMapper;

mod error;
mod from_openapi;
mod generator;
mod scaffold;
mod schema_gen;
mod sync;
mod test_gen;
mod to_docs_json;
mod to_openapi;

#[derive(Parser, Debug)]
#[clap(author, version, about = "CDD Toolchain CLI")]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
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
}

fn main() -> AppResult<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Sync(args) => {
            // Injecting Diesel/dsync mapper
            let mapper = DieselMapper;
            sync::execute(args, &mapper)?;
        }
        Commands::TestGen(args) => {
            // Injecting Actix Web strategy
            let strategy = ActixStrategy;
            test_gen::execute(args, &strategy)?;
        }
        Commands::Scaffold(args) => {
            // Injecting Actix Web strategy
            let strategy = ActixStrategy;
            scaffold::execute(args, &strategy)?;
        }
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
            to_openapi::execute(args)?;
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
