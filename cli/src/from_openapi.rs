//! Module for generating scaffolding from OpenAPI specifications.
use cdd_core::error::{AppError, AppResult};
use cdd_core::strategies::{ClapCliStrategy, ReqwestStrategy};
use clap::{Args, Subcommand};
use std::env;
use std::fs;
use std::path::PathBuf;

use crate::scaffold::ScaffoldArgs;
use crate::test_gen::TestGenArgs;

/// Available server frameworks.
#[derive(clap::ValueEnum, Clone, Debug, Default, PartialEq, Eq)]
pub enum ServerFramework {
    /// Actix Web framework
    #[default]
    ActixWeb,
    /// Axum framework
    Axum,
}

/// Arguments for generating SDKs or Server scaffolding from an OpenAPI spec.
#[derive(Args, Debug)]
pub struct FromOpenApiArgs {
    /// The specific generation command to run.
    #[clap(subcommand)]
    pub command: FromOpenApiCommands,
}

/// Commands available under the `from_openapi` subcommand.
#[derive(Subcommand, Debug)]
pub enum FromOpenApiCommands {
    /// Generate a CLI SDK
    #[clap(name = "to_sdk_cli")]
    SdkCli(GenerateArgs),
    /// Generate a Client SDK
    #[clap(name = "to_sdk")]
    Sdk(GenerateArgs),
    /// Generate Server scaffolding
    #[clap(name = "to_server")]
    Server {
        /// The common arguments for generation
        #[clap(flatten)]
        args: GenerateArgs,
        /// The target server framework (actix-web or axum). Defaults to actix-web.
        #[clap(long, default_value = "actix-web", env = "CDD_SERVER_FRAMEWORK")]
        framework: ServerFramework,
    },
}

/// Common arguments used for generation commands.
#[derive(Args, Debug)]
pub struct GenerateArgs {
    /// Path or URL to the OpenAPI specification.
    #[clap(short, long, required_unless_present = "input_dir", env = "CDD_INPUT")]
    pub input: Option<PathBuf>,

    /// Directory containing OpenAPI specifications.
    #[clap(long, required_unless_present = "input", env = "CDD_INPUT_DIR")]
    pub input_dir: Option<PathBuf>,

    /// Output directory for generated code. Defaults to current directory.
    #[clap(short, long, env = "CDD_OUTPUT_DIR")]
    pub output_dir: Option<PathBuf>,

    /// Do not generate GitHub Actions scaffolding.
    #[clap(long, env = "CDD_NO_GITHUB_ACTIONS")]
    pub no_github_actions: bool,

    /// Do not generate an installable package scaffolding (e.g. Cargo.toml).
    #[clap(long, env = "CDD_NO_INSTALLABLE_PACKAGE")]
    pub no_installable_package: bool,

    /// Generate integration tests.
    #[clap(long, env = "CDD_TESTS")]
    pub tests: bool,
}

impl GenerateArgs {
    /// Get the output directory, defaulting to the current directory.
    pub fn get_output_dir(&self) -> PathBuf {
        self.output_dir
            .clone()
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

    /// Retrieve the input files either from the single file or the directory specified.
    pub fn get_input_files(&self) -> Vec<PathBuf> {
        if let Some(ref file) = self.input {
            vec![file.clone()]
        } else if let Some(ref dir) = self.input_dir {
            // collect all json/yaml in dir
            let mut files = Vec::new();
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(ext) = path.extension() {
                            if ext == "json" || ext == "yaml" || ext == "yml" {
                                files.push(path);
                            }
                        }
                    }
                }
            }
            files
        } else {
            vec![]
        }
    }
}

/// Executes the requested OpenAPI generation command.
pub fn execute(args: &FromOpenApiArgs) -> AppResult<()> {
    match &args.command {
        FromOpenApiCommands::SdkCli(gen_args) => {
            println!("Generating SDK CLI...");
            run_generation(gen_args, &ClapCliStrategy)?;
        }
        FromOpenApiCommands::Sdk(gen_args) => {
            println!("Generating SDK...");
            run_generation(gen_args, &ReqwestStrategy)?;
        }
        FromOpenApiCommands::Server {
            args: gen_args,
            framework,
        } => {
            println!("Generating Server with framework {:?}...", framework);
            match framework {
                ServerFramework::ActixWeb => {
                    run_generation(gen_args, &cdd_core::strategies::ActixStrategy)?
                }
                ServerFramework::Axum => {
                    run_generation(gen_args, &cdd_core::strategies::AxumStrategy)?
                }
            }
        }
    }
    Ok(())
}

/// Runs the generation using the specified strategy.
fn run_generation(
    args: &GenerateArgs,
    strategy: &impl cdd_core::strategies::BackendStrategy,
) -> AppResult<()> {
    let inputs = args.get_input_files();
    if inputs.is_empty() {
        return Err(AppError::General(
            "No input files provided or found.".into(),
        ));
    }

    let out_dir = args.get_output_dir();
    fs::create_dir_all(&out_dir).unwrap_or_default();

    if !args.no_installable_package {
        let cargo_toml_path = out_dir.join("Cargo.toml");
        if !cargo_toml_path.exists() {
            let cargo_toml = format!(
                r#"[package]
name = "{}"
version = "0.0.1"
edition = "2021"

[dependencies]
tokio = {{ version = "1", features = ["full"] }}
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"
serde_qs = "1.0"
reqwest = {{ version = "0.11", features = ["json"] }}
actix-web = "4.0"
diesel = {{ version = "2.0", features = ["postgres"] }}
clap = {{ version = "4.0", features = ["derive"] }}
chrono = {{ version = "0.4", features = ["serde"] }}
utoipa = "4.2"
uuid = {{ version = "1.8", features = ["serde"] }}
"#,
                out_dir
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("generated_package"))
                    .to_string_lossy()
            );
            fs::write(cargo_toml_path, cargo_toml).unwrap_or_default();
        }
    }

    if !args.no_github_actions {
        let gh_workflows_dir = out_dir.join(".github").join("workflows");
        fs::create_dir_all(&gh_workflows_dir).unwrap_or_default();
        let ci_yml_path = gh_workflows_dir.join("ci.yml");
        if !ci_yml_path.exists() {
            let ci_yml = r#"name: CI
on:
  push:
    branches: [ main ]
  pull_request:
    branches: [ main ]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Build
        run: cargo build --verbose
      - name: Run tests
        run: cargo test --verbose
"#;
            fs::write(ci_yml_path, ci_yml).unwrap_or_default();
        }
    }

    for input in inputs {
        let yaml_content = fs::read_to_string(&input)
            .map_err(|e| AppError::General(format!("Failed to read OpenAPI {:?}: {}", input, e)))?;

        // 1. Generate Models (DTOs)
        let src_dir = out_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap_or_default();
        let mut lib_rs_content = String::new();

        let models_dir = src_dir.join("models");
        fs::create_dir_all(&models_dir).unwrap_or_default();

        let models = cdd_core::openapi::parse::parse_openapi_spec(&yaml_content)?;
        if !models.is_empty() {
            let rust_code = cdd_core::classes::emit::generate_dtos(&models);
            let file_path = models_dir.join("generated.rs");
            fs::write(&file_path, rust_code).unwrap_or_default();
            let mod_rs = "pub mod generated;\npub use generated::*;\n";
            fs::write(models_dir.join("mod.rs"), mod_rs).unwrap_or_default();
            lib_rs_content.push_str("pub mod models;\n");
        }

        // 2. Scaffold Handlers
        let handlers_dir = src_dir.join("handlers");
        fs::create_dir_all(&handlers_dir).unwrap_or_default();
        let scaffold_args = ScaffoldArgs {
            openapi_path: input.clone(),
            output_dir: handlers_dir.clone(),
            route_config_path: None,
            force: false,
        };
        crate::scaffold::execute(&scaffold_args, strategy)?;

        // Gather handler modules for mod.rs
        if handlers_dir.exists() {
            let mut mods = Vec::new();
            if let Ok(entries) = fs::read_dir(&handlers_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file()
                        && path.extension().unwrap_or_default() == "rs"
                        && path.file_name().unwrap_or_default() != "mod.rs"
                    {
                        let mod_name = path.file_stem().unwrap_or_default().to_string_lossy();
                        mods.push(format!("pub mod {};\n", mod_name));
                    }
                }
            }
            if !mods.is_empty() {
                mods.sort();
                fs::write(handlers_dir.join("mod.rs"), mods.join("")).unwrap_or_default();
                lib_rs_content.push_str("pub mod handlers;\n");
            }
        }

        if !lib_rs_content.is_empty() {
            fs::write(src_dir.join("lib.rs"), lib_rs_content).unwrap_or_default();
        }

        // 3. Generate Tests
        if args.tests {
            let tests_dir = out_dir.join("tests");
            fs::create_dir_all(&tests_dir).unwrap_or_default();
            let crate_name = out_dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .replace('-', "_");
            let test_gen_args = TestGenArgs {
                openapi_path: input.clone(),
                output_path: tests_dir.join("api_contracts.rs"),
                app_factory: crate_name,
            };
            crate::test_gen::execute(&test_gen_args, strategy)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_get_output_dir_default() {
        let args = GenerateArgs {
            input: None,
            input_dir: None,
            output_dir: None,
            no_github_actions: false,
            no_installable_package: false,
            tests: false,
        };
        let dir = args.get_output_dir();
        assert_eq!(
            dir,
            env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        );
    }

    #[test]
    fn test_get_output_dir_provided() {
        let path = PathBuf::from("/tmp/out");
        let args = GenerateArgs {
            input: None,
            input_dir: None,
            output_dir: Some(path.clone()),
            no_github_actions: false,
            no_installable_package: false,
            tests: false,
        };
        assert_eq!(args.get_output_dir(), path);
    }

    #[test]
    fn test_get_input_files_single() {
        let path = PathBuf::from("spec.yaml");
        let args = GenerateArgs {
            input: Some(path.clone()),
            input_dir: None,
            output_dir: None,
            no_github_actions: false,
            no_installable_package: false,
            tests: false,
        };
        assert_eq!(args.get_input_files(), vec![path]);
    }

    #[test]
    fn test_get_input_files_dir() {
        let dir = tempdir().expect("Failed to create temporary directory");
        let file1 = dir.path().join("a.yaml");
        let file2 = dir.path().join("b.json");
        let file3 = dir.path().join("c.txt");
        fs::write(&file1, "").expect("Failed to write to file");
        fs::write(&file2, "").expect("Failed to write to file");
        fs::write(&file3, "").expect("Failed to write to file");

        let args = GenerateArgs {
            input: None,
            input_dir: Some(dir.path().to_path_buf()),
            output_dir: None,
            no_github_actions: false,
            no_installable_package: false,
            tests: false,
        };
        let mut files = args.get_input_files();
        files.sort();
        let mut expected = vec![file1, file2];
        expected.sort();
        assert_eq!(files, expected);
    }

    #[test]
    fn test_get_input_files_none() {
        let args = GenerateArgs {
            input: None,
            input_dir: None,
            output_dir: None,
            no_github_actions: false,
            no_installable_package: false,
            tests: false,
        };
        assert!(args.get_input_files().is_empty());
    }

    #[test]
    fn test_execute_and_run_generation() {
        let dir = tempdir().expect("Failed to create temporary directory");
        let input_file = dir.path().join("spec.yaml");
        let openapi_content = r#"
openapi: 3.0.0
info:
  title: Test API
  version: 1.0.0
paths:
  /test:
    get:
      operationId: test_op
      responses:
        '200':
          description: OK
components:
  schemas:
    Pet:
      type: object
      properties:
        id:
          type: integer
          format: int64
"#;
        fs::write(&input_file, openapi_content).expect("Failed to write to file");

        // Test SdkCli
        let args = FromOpenApiArgs {
            command: FromOpenApiCommands::SdkCli(GenerateArgs {
                input: Some(input_file.clone()),
                input_dir: None,
                output_dir: Some(dir.path().join("out1")),
                no_github_actions: false,
                no_installable_package: false,
                tests: false,
            }),
        };
        assert!(execute(&args).is_ok());

        // Test Sdk
        let args = FromOpenApiArgs {
            command: FromOpenApiCommands::Sdk(GenerateArgs {
                input: Some(input_file.clone()),
                input_dir: None,
                output_dir: Some(dir.path().join("out2")),
                no_github_actions: false,
                no_installable_package: false,
                tests: false,
            }),
        };
        assert!(execute(&args).is_ok());

        // Test Server with ActixWeb
        let args = FromOpenApiArgs {
            command: FromOpenApiCommands::Server {
                args: GenerateArgs {
                    input: Some(input_file.clone()),
                    input_dir: None,
                    output_dir: Some(dir.path().join("out3")),
                    no_github_actions: false,
                    no_installable_package: false,
                    tests: false,
                },
                framework: ServerFramework::ActixWeb,
            },
        };
        assert!(execute(&args).is_ok());

        // Test Server with Axum
        let args = FromOpenApiArgs {
            command: FromOpenApiCommands::Server {
                args: GenerateArgs {
                    input: Some(input_file.clone()),
                    input_dir: None,
                    output_dir: Some(dir.path().join("out4")),
                    no_github_actions: false,
                    no_installable_package: false,
                    tests: false,
                },
                framework: ServerFramework::Axum,
            },
        };
        assert!(execute(&args).is_ok());
    }

    #[test]
    fn test_run_generation_empty() {
        let args = GenerateArgs {
            input: None,
            input_dir: None,
            output_dir: None,
            no_github_actions: false,
            no_installable_package: false,
            tests: false,
        };
        let result = run_generation(&args, &cdd_core::strategies::ActixStrategy);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_generation_invalid_file() {
        let args = GenerateArgs {
            input: Some(PathBuf::from("/does/not/exist.yaml")),
            input_dir: None,
            output_dir: None,
            no_github_actions: false,
            no_installable_package: false,
            tests: false,
        };
        let result = run_generation(&args, &cdd_core::strategies::ActixStrategy);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_generation_with_tests_flag() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let input = dir.path().join("spec.json");
        std::fs::write(
            &input,
            r#"{"openapi": "3.0.0", "info": {"title": "Test", "version": "1.0"}, "paths": {}}"#,
        )?;
        let args = GenerateArgs {
            input: Some(input),
            input_dir: None,
            output_dir: Some(dir.path().join("my_out_app")),
            no_github_actions: true,
            no_installable_package: true,
            tests: true,
        };
        let _ = run_generation(&args, &cdd_core::strategies::ActixStrategy);
        Ok(())
    }
}
