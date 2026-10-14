#![deny(missing_docs)]

//! # Clap CLI Strategy Module
//!
//! Implementation of [`CodegenStrategy`], [`CliStrategy`], and [`BackendStrategy`]
//! for generating a pure Rust CLI using `clap` and `reqwest`.

pub mod extractors;
pub mod registration;
pub mod scaffolding;
pub mod testing;

use crate::openapi::parse::ParsedRoute;
use crate::strategies::{BackendStrategy, CliStrategy, CodegenStrategy, StrategyKind};

/// Strategy for generating Clap CLI code.
pub struct ClapCliStrategy;

impl CodegenStrategy for ClapCliStrategy {
    fn kind(&self) -> StrategyKind {
        StrategyKind::Cli
    }

    fn module_imports(&self) -> String {
        scaffolding::handler_imports()
    }

    fn emit_handler(&self, route: &ParsedRoute, args: &[String]) -> String {
        scaffolding::handler_signature(route, args)
    }
}

impl CliStrategy for ClapCliStrategy {
    fn generate_cli_main(&self, module_subdir: &str, module_names: &[String]) -> String {
        let mut main_rs = String::new();
        main_rs.push_str(
            "#![allow(missing_docs)]

",
        );
        main_rs.push_str(
            "use clap::{Parser, Subcommand};
",
        );
        main_rs.push_str(
            "use reqwest::Client;

",
        );
        main_rs.push_str(&format!(
            "use generated_package::{}::*;

",
            module_subdir
        ));

        main_rs.push_str(
            "#[derive(Parser, Debug)]
",
        );
        main_rs.push_str(r#"#[clap(author, version, about = "Generated CLI")]"#);
        main_rs.push(char::from(10u8));
        main_rs.push_str(
            "struct Cli {
",
        );
        main_rs.push_str(r#"    #[clap(long, default_value = "http://localhost:8080")]"#);
        main_rs.push(char::from(10u8));
        main_rs.push_str(
            "    base_url: String,
",
        );
        main_rs.push_str(
            "    #[clap(subcommand)]
",
        );
        main_rs.push_str(
            "    command: Commands,
",
        );
        main_rs.push_str(
            "}

",
        );

        main_rs.push_str(
            "#[derive(Subcommand, Debug)]
",
        );
        main_rs.push_str(
            "enum Commands {
",
        );

        let mut variants = Vec::new();
        for mod_name in module_names {
            let struct_name = mod_name
                .split('_')
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let mut c = s.chars();
                    match c.next() {
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                        None => String::new(),
                    }
                })
                .collect::<String>();
            variants.push((mod_name.clone(), struct_name));
        }

        for (mod_name, struct_name) in &variants {
            main_rs.push_str(&format!(
                "    {}(generated_package::{}::{}::{}Args),
",
                struct_name, module_subdir, mod_name, struct_name
            ));
        }
        main_rs.push_str(
            "    /// Run as an MCP server over stdio
",
        );
        main_rs.push_str(
            "    Mcp,
",
        );
        main_rs.push_str(
            "}

",
        );

        main_rs.push_str(
            "#[tokio::main]
",
        );
        main_rs.push_str(
            "async fn main() -> Result<(), Box<dyn std::error::Error>> {
",
        );
        main_rs.push_str(
            "    let cli = Cli::parse();
",
        );
        main_rs.push_str(
            "    let client = Client::new();

",
        );
        main_rs.push_str(
            "    match cli.command {
",
        );
        for (mod_name, struct_name) in &variants {
            main_rs.push_str(&format!(
                "        Commands::{}(args) => {{
",
                struct_name
            ));
            main_rs.push_str(&format!(
                "            let res = generated_package::{}::{}::{}(args, &client, &cli.base_url).await?;
",
                module_subdir, mod_name, mod_name
            ));
            main_rs.push_str(r#"            println!("{:?}", res);"#);
            main_rs.push(char::from(10u8));
            main_rs.push_str(
                "        }
",
            );
        }
        main_rs.push_str(
            "        Commands::Mcp => {
",
        );
        main_rs.push_str(
            r#"            println!("MCP Server not yet fully implemented for generated CLI");"#,
        );
        main_rs.push(char::from(10u8));
        main_rs.push_str(
            "        }
",
        );
        main_rs.push_str(
            "    }

",
        );
        main_rs.push_str(
            "    Ok(())
",
        );
        main_rs.push_str(
            "}
",
        );
        main_rs
    }

    fn cli_command_test(&self, route: &ParsedRoute, crate_name: &str) -> String {
        testing::generate_custom_test(route, crate_name)
    }
}

impl BackendStrategy for ClapCliStrategy {
    fn extra_handler_docs(&self, route: &ParsedRoute) -> String {
        format!(
            "/// @OAS_METHOD: {}
/// @OAS_PATH: {}
",
            route.method, route.path
        )
    }

    fn test_imports(&self) -> String {
        testing::test_imports()
    }

    fn test_fn_signature(&self, fn_name: &str) -> String {
        testing::test_fn_signature(fn_name)
    }

    fn test_app_init(&self, app_factory: &str) -> String {
        testing::test_app_init(app_factory)
    }

    fn test_body_setup_code(&self, body: &crate::openapi::parse::RequestBodyDefinition) -> String {
        testing::test_body_setup_code(body)
    }

    fn test_request_builder(&self, method: &str, uri: &str, body_setup: &str) -> String {
        testing::test_request_builder(method, uri, body_setup)
    }

    fn test_api_call(&self) -> String {
        testing::test_api_call()
    }

    fn test_assertion(&self) -> String {
        testing::test_assertion()
    }

    fn test_validation_helper(&self) -> String {
        testing::test_validation_helper()
    }

    fn generate_custom_test(&self, route: &ParsedRoute, app_factory: &str) -> Option<String> {
        Some(self.cli_command_test(route, app_factory))
    }
}
