#![deny(missing_docs)]

//! Module for generating Rust Client SDKs, Clap CLIs (reusing the SDK), and Backend Server
//! orchestrators/E2E tests from Arazzo 1.1.0 specifications.

use crate::from_openapi::{read_spec_source, ServerFramework};
use cdd_core::arazzo::parse::{parse_arazzo_document_with_registry, resolve_source_descriptions};
use cdd_core::error::{AppError, AppResult};
use cdd_core::openapi::parse::registry::DocumentRegistry;
use cdd_core::strategies::{
    ActixStrategy, AxumStrategy, ClapCliStrategy, ReqwestStrategy, WorkflowStrategy,
};
use clap::{Args, Subcommand};
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

/// Strongly typed subcommand for `from_arazzo` programmatic configuration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FromArazzoSubcommand {
    /// Generate a Reqwest Client SDK (`to_sdk`).
    #[default]
    ToSdk,
    /// Generate a Client SDK and Clap CLI binary reusing the SDK (`to_sdk_cli`).
    ToSdkCli,
    /// Generate an HTTP Server workflow module and E2E test suite (`to_server`).
    ToServer,
}

impl FromStr for FromArazzoSubcommand {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "to_sdk" => Ok(Self::ToSdk),
            "to_sdk_cli" => Ok(Self::ToSdkCli),
            "to_server" => Ok(Self::ToServer),
            _ => Err(AppError::General(format!(
                "Unknown from_arazzo subcommand: {}",
                s
            ))),
        }
    }
}

/// CLI arguments for the `from_arazzo` command.
#[derive(Args, Debug)]
pub struct FromArazzoArgs {
    /// Target generation subcommand (`to_sdk`, `to_sdk_cli`, or `to_server`).
    #[clap(subcommand)]
    pub command: FromArazzoCommands,
}

/// Subcommands available under `from_arazzo`.
#[derive(Subcommand, Debug)]
pub enum FromArazzoCommands {
    /// Generate a Clap CLI workflow runner that reuses the generated Client SDK.
    #[clap(name = "to_sdk_cli")]
    SdkCli(ArazzoGenerateArgs),
    /// Generate a Reqwest Client SDK workflow orchestrator.
    #[clap(name = "to_sdk")]
    Sdk(ArazzoGenerateArgs),
    /// Generate Server workflow orchestrators and multi-step E2E integration tests.
    #[clap(name = "to_server")]
    Server {
        /// Common generation arguments.
        #[clap(flatten)]
        args: ArazzoGenerateArgs,
        /// Target HTTP server framework (`actix-web` or `axum`).
        #[clap(long, default_value = "actix-web", env = "CDD_SERVER_FRAMEWORK")]
        framework: ServerFramework,
    },
}

/// Common arguments for `from_arazzo` targets.
#[derive(Args, Debug, Clone, Default)]
pub struct ArazzoGenerateArgs {
    /// Path or URL to the Arazzo 1.1.0 specification.
    #[clap(
        short = 'i',
        long,
        required_unless_present = "input_dir",
        env = "CDD_INPUT"
    )]
    pub input: Option<PathBuf>,

    /// Directory containing Arazzo 1.1.0 specifications.
    #[clap(
        short = 'd',
        long,
        required_unless_present = "input",
        env = "CDD_INPUT_DIR"
    )]
    pub input_dir: Option<PathBuf>,

    /// Output directory path.
    #[clap(short = 'o', long = "output", env = "CDD_OUTPUT")]
    pub output_dir: Option<PathBuf>,

    /// Optional directory for resolving relative OpenAPI `sourceDescriptions`.
    #[clap(long, env = "CDD_OPENAPI_DIR")]
    pub openapi_dir: Option<PathBuf>,

    /// Do not generate GitHub Actions CI scaffolding.
    #[clap(long, env = "CDD_NO_GITHUB_ACTIONS")]
    pub no_github_actions: bool,

    /// Do not generate `Cargo.toml` package scaffolding.
    #[clap(long, env = "CDD_NO_INSTALLABLE_PACKAGE")]
    pub no_installable_package: bool,

    /// Generate E2E workflow integration tests.
    #[clap(long, env = "CDD_TESTS")]
    pub tests: bool,

    /// Generate Model Context Protocol (MCP) workflow bindings.
    #[clap(short = 'm', long, env = "CDD_MCP")]
    pub mcp: bool,
}

impl ArazzoGenerateArgs {
    /// Resolves the output directory, defaulting to `.`.
    pub fn get_output_dir(&self) -> PathBuf {
        self.output_dir.clone().unwrap_or(PathBuf::from("."))
    }

    /// Collects all Arazzo specification files from `input` or `input_dir`.
    pub fn get_input_files(&self) -> Vec<PathBuf> {
        if let Some(ref f) = self.input {
            return vec![f.clone()];
        }
        let mut files = Vec::new();
        if let Some(ref dir) = self.input_dir {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let is_spec = matches!(
                        path.extension().and_then(|e| e.to_str()),
                        Some("json" | "yaml" | "yml")
                    );
                    if path.is_file() && is_spec {
                        files.push(path);
                    }
                }
            }
        }
        files
    }
}

/// Executes the `from_arazzo` CLI command.
///
/// # Arguments
/// * `args` - Parsed [`FromArazzoArgs`].
///
/// # Returns
/// `Ok(())` on success.
///
/// # Errors
/// Returns [`AppError`] if reading specifications or emitting files fails.
pub fn run_from_arazzo(args: &FromArazzoArgs) -> AppResult<()> {
    match &args.command {
        FromArazzoCommands::Sdk(gen_args) => {
            generate_arazzo_target(gen_args, &ReqwestStrategy, false)
        }
        FromArazzoCommands::SdkCli(gen_args) => {
            generate_arazzo_target(gen_args, &ClapCliStrategy, true)
        }
        FromArazzoCommands::Server {
            args: gen_args,
            framework,
        } => match framework {
            ServerFramework::ActixWeb => generate_arazzo_target(gen_args, &ActixStrategy, false),
            ServerFramework::Axum => generate_arazzo_target(gen_args, &AxumStrategy, false),
        },
    }
}

fn generate_arazzo_target(
    args: &ArazzoGenerateArgs,
    strategy: &dyn WorkflowStrategy,
    is_cli: bool,
) -> AppResult<()> {
    let inputs = args.get_input_files();
    if inputs.is_empty() {
        return Err(AppError::General(
            "No Arazzo specification files found to process.".to_string(),
        ));
    }

    let out_dir = args.get_output_dir();
    let src_dir = out_dir.join("src");
    fs::create_dir_all(&src_dir)?;

    if !args.no_installable_package {
        let cargo_path = out_dir.join("Cargo.toml");
        if !cargo_path.exists() {
            fs::write(
                &cargo_path,
                r#"[package]
name = "generated_package"
version = "0.1.0"
edition = "2021"

[dependencies]
clap = { version = "4", features = ["derive"] }
regex = "1"
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1", features = ["full"] }
"#,
            )?;
        }
    }

    if !args.no_github_actions {
        let gh_dir = out_dir.join(".github").join("workflows");
        fs::create_dir_all(&gh_dir)?;
        let ci_file = gh_dir.join("ci.yml");
        if !ci_file.exists() {
            fs::write(
                &ci_file,
                "name: CI
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo check
",
            )?;
        }
    }

    let mut registry = DocumentRegistry::new();
    for input_path in &inputs {
        let content = read_spec_source(input_path)?;
        let base_dir = match &args.openapi_dir {
            Some(d) => Some(d.as_path()),
            None => input_path.parent(),
        };

        let doc = parse_arazzo_document_with_registry(&content, None, &mut registry)?;
        let linked = resolve_source_descriptions(&doc, base_dir, &mut registry)?;

        let wf_code = strategy.emit_workflow_module(&doc, &linked)?;
        fs::write(src_dir.join("workflows.rs"), wf_code)?;

        let lib_rs = src_dir.join("lib.rs");
        let existing_lib =
            fs::read_to_string(&lib_rs).unwrap_or_else(|_| "#![allow(missing_docs)]\n".to_string());
        if !existing_lib.contains("pub mod workflows;") {
            fs::write(
                &lib_rs,
                format!("{}\npub mod workflows;\n", existing_lib.trim_end()),
            )?;
        }

        if is_cli {
            let main_rs = src_dir.join("main.rs");
            if !main_rs.exists() {
                fs::write(
                    &main_rs,
                    r#"#![allow(missing_docs)]

use clap::Parser;
use generated_package::workflows::{execute_workflow_cli, WorkflowCommands, WorkflowsClient};

#[derive(Parser, Debug)]
struct Cli {
    #[clap(subcommand)]
    command: WorkflowCommands,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let client = WorkflowsClient::new(reqwest::Client::new());
    let out = execute_workflow_cli(cli.command, &client).await?;
    println!("{}", out);
    Ok(())
}
"#,
                )?;
            }
        }

        if args.tests {
            let tests_dir = out_dir.join("tests");
            fs::create_dir_all(&tests_dir)?;
            let test_code = strategy.emit_workflow_tests(&doc, "generated_package")?;
            fs::write(tests_dir.join("arazzo_workflows.rs"), test_code)?;
        }
    }

    Ok(())
}

/// Programmatic configuration for `from_arazzo`.
#[derive(Debug, Default, Clone)]
pub struct FromArazzoConfig {
    /// Subcommand target (`ToSdk`, `ToSdkCli`, `ToServer`).
    pub subcommand: FromArazzoSubcommand,
    /// Input Arazzo file path.
    pub input: Option<PathBuf>,
    /// Input directory containing Arazzo files.
    pub input_dir: Option<PathBuf>,
    /// Output directory path.
    pub output_dir: Option<PathBuf>,
    /// Optional directory for resolving relative OpenAPI `sourceDescriptions`.
    pub openapi_dir: Option<PathBuf>,
    /// Server framework when `subcommand` is `ToServer`.
    pub framework: ServerFramework,
    /// Disable GitHub Actions generation.
    pub no_github_actions: bool,
    /// Disable Cargo.toml generation.
    pub no_installable_package: bool,
    /// Generate E2E workflow tests.
    pub tests: bool,
    /// Generate MCP workflow module.
    pub mcp: bool,
}

/// Programmatic entrypoint for generating Rust code from an Arazzo 1.1.0 specification.
///
/// # Arguments
/// * `config` - [`FromArazzoConfig`] parameters.
///
/// # Returns
/// `Ok(())` on success.
///
/// # Errors
/// Returns [`AppError`] if parsing or code generation fails.
pub fn from_arazzo(config: &FromArazzoConfig) -> AppResult<()> {
    let gen_args = ArazzoGenerateArgs {
        input: config.input.clone(),
        input_dir: config.input_dir.clone(),
        output_dir: config.output_dir.clone(),
        openapi_dir: config.openapi_dir.clone(),
        no_github_actions: config.no_github_actions,
        no_installable_package: config.no_installable_package,
        tests: config.tests,
        mcp: config.mcp,
    };
    let command = match config.subcommand {
        FromArazzoSubcommand::ToSdk => FromArazzoCommands::Sdk(gen_args),
        FromArazzoSubcommand::ToSdkCli => FromArazzoCommands::SdkCli(gen_args),
        FromArazzoSubcommand::ToServer => FromArazzoCommands::Server {
            args: gen_args,
            framework: config.framework,
        },
    };
    run_from_arazzo(&FromArazzoArgs { command })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_from_arazzo_sdk_cli_and_server_targets() {
        assert_eq!(
            "to_sdk".parse::<FromArazzoSubcommand>().expect("to_sdk"),
            FromArazzoSubcommand::ToSdk
        );
        assert_eq!(
            "to_sdk_cli"
                .parse::<FromArazzoSubcommand>()
                .expect("to_sdk_cli"),
            FromArazzoSubcommand::ToSdkCli
        );
        assert_eq!(
            "to_server"
                .parse::<FromArazzoSubcommand>()
                .expect("to_server"),
            FromArazzoSubcommand::ToServer
        );
        assert!("bad".parse::<FromArazzoSubcommand>().is_err());

        let default_args = ArazzoGenerateArgs::default();
        assert_eq!(default_args.get_output_dir(), PathBuf::from("."));
        assert!(default_args.get_input_files().is_empty());
        let missing_dir_args = ArazzoGenerateArgs {
            input_dir: Some(PathBuf::from("/nonexistent_arazzo_dir_xyz")),
            ..Default::default()
        };
        assert!(missing_dir_args.get_input_files().is_empty());

        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("notes.txt"), "ignore me").expect("write txt");
        fs::write(dir.path().join("no_ext"), "ignore me").expect("write no_ext");
        let spec_file = dir.path().join("petstore.arazzo.yaml");
        fs::write(
            &spec_file,
            r#"
arazzo: 1.1.0
info:
  title: Petstore
  version: 1.0.0
sourceDescriptions:
  - name: petstore
    url: ./petstore.yaml
    type: openapi
workflows:
  - workflowId: adoptPet
    steps:
      - stepId: findPet
        operationId: getPet
"#,
        )
        .expect("write spec");

        let sdk_out = dir.path().join("sdk_out");
        fs::create_dir_all(sdk_out.join("src")).expect("mkdir sdk_out/src");
        fs::write(sdk_out.join("src/lib.rs"), "#![allow(missing_docs)]\n")
            .expect("pre-create lib.rs");
        let cfg_sdk = FromArazzoConfig {
            subcommand: FromArazzoSubcommand::ToSdk,
            input: Some(spec_file.clone()),
            output_dir: Some(sdk_out.clone()),
            tests: true,
            ..Default::default()
        };
        from_arazzo(&cfg_sdk).expect("from_arazzo sdk");
        assert!(sdk_out.join("src/workflows.rs").exists());
        assert!(sdk_out.join("tests/arazzo_workflows.rs").exists());

        let cli_out = dir.path().join("cli_out");
        let cfg_cli = FromArazzoConfig {
            subcommand: FromArazzoSubcommand::ToSdkCli,
            input_dir: Some(dir.path().to_path_buf()),
            output_dir: Some(cli_out.clone()),
            openapi_dir: Some(dir.path().to_path_buf()),
            tests: true,
            ..Default::default()
        };
        from_arazzo(&cfg_cli).expect("from_arazzo cli");
        from_arazzo(&cfg_cli).expect("from_arazzo cli second pass");
        assert!(cli_out.join("src/main.rs").exists());

        let actix_out = dir.path().join("actix_out");
        let cfg_actix = FromArazzoConfig {
            subcommand: FromArazzoSubcommand::ToServer,
            input: Some(spec_file.clone()),
            output_dir: Some(actix_out.clone()),
            framework: ServerFramework::ActixWeb,
            tests: true,
            ..Default::default()
        };
        from_arazzo(&cfg_actix).expect("from_arazzo actix");
        assert!(actix_out.join("tests/arazzo_workflows.rs").exists());

        let axum_out = dir.path().join("axum_out");
        let cfg_axum = FromArazzoConfig {
            subcommand: FromArazzoSubcommand::ToServer,
            input: Some(spec_file),
            output_dir: Some(axum_out.clone()),
            framework: ServerFramework::Axum,
            no_github_actions: true,
            no_installable_package: true,
            tests: true,
            ..Default::default()
        };
        from_arazzo(&cfg_axum).expect("from_arazzo axum");
        assert!(axum_out.join("tests/arazzo_workflows.rs").exists());

        let empty_cfg = FromArazzoConfig::default();
        assert!(from_arazzo(&empty_cfg).is_err());
    }
}
