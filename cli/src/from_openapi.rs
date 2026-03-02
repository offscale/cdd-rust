use cdd_core::error::{AppError, AppResult};
use cdd_core::strategies::{ActixStrategy, ClapCliStrategy, ReqwestStrategy};
use clap::{Args, Subcommand};
use std::env;
use std::fs;
use std::path::PathBuf;

use crate::scaffold::ScaffoldArgs;
use crate::test_gen::TestGenArgs;

#[derive(Args, Debug)]
pub struct FromOpenApiArgs {
    #[clap(subcommand)]
    pub command: FromOpenApiCommands,
}

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
    Server(GenerateArgs),
}

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
}

impl GenerateArgs {
    pub fn get_output_dir(&self) -> PathBuf {
        self.output_dir
            .clone()
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

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
        FromOpenApiCommands::Server(gen_args) => {
            println!("Generating Server...");
            run_generation(gen_args, &ActixStrategy)?;
        }
    }
    Ok(())
}

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

    for input in inputs {
        let yaml_content = fs::read_to_string(&input)
            .map_err(|e| AppError::General(format!("Failed to read OpenAPI {:?}: {}", input, e)))?;

        // 1. Generate Models (DTOs)
        let models_dir = out_dir.join("models");
        fs::create_dir_all(&models_dir).unwrap_or_default();

        let models = cdd_core::openapi::parse::parse_openapi_spec(&yaml_content)?;
        if !models.is_empty() {
            let rust_code = cdd_core::classes::emit::generate_dtos(&models);
            let file_path = models_dir.join("generated.rs");
            fs::write(&file_path, rust_code).unwrap_or_default();
            let mod_rs = "pub mod generated;\npub use generated::*;\n";
            fs::write(models_dir.join("mod.rs"), mod_rs).unwrap_or_default();
        }

        // 2. Scaffold Handlers
        let handlers_dir = out_dir.join("handlers");
        let scaffold_args = ScaffoldArgs {
            openapi_path: input.clone(),
            output_dir: handlers_dir,
            route_config_path: None,
            force: false,
        };
        crate::scaffold::execute(&scaffold_args, strategy)?;

        // 3. Generate Tests
        let tests_dir = out_dir.join("tests");
        fs::create_dir_all(&tests_dir).unwrap_or_default();
        let test_gen_args = TestGenArgs {
            openapi_path: input.clone(),
            output_path: tests_dir.join("api_contracts.rs"),
            app_factory: "crate::create_app".to_string(),
        };
        crate::test_gen::execute(&test_gen_args, strategy)?;
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
        };
        assert_eq!(args.get_input_files(), vec![path]);
    }

    #[test]
    fn test_get_input_files_dir() {
        let dir = tempdir().unwrap();
        let file1 = dir.path().join("a.yaml");
        let file2 = dir.path().join("b.json");
        let file3 = dir.path().join("c.txt");
        fs::write(&file1, "").unwrap();
        fs::write(&file2, "").unwrap();
        fs::write(&file3, "").unwrap();

        let args = GenerateArgs {
            input: None,
            input_dir: Some(dir.path().to_path_buf()),
            output_dir: None,
            no_github_actions: false,
            no_installable_package: false,
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
        };
        assert!(args.get_input_files().is_empty());
    }

    #[test]
    fn test_execute_and_run_generation() {
        let dir = tempdir().unwrap();
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
        fs::write(&input_file, openapi_content).unwrap();

        // Test SdkCli
        let args = FromOpenApiArgs {
            command: FromOpenApiCommands::SdkCli(GenerateArgs {
                input: Some(input_file.clone()),
                input_dir: None,
                output_dir: Some(dir.path().join("out1")),
                no_github_actions: false,
                no_installable_package: false,
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
            }),
        };
        assert!(execute(&args).is_ok());

        // Test Server
        let args = FromOpenApiArgs {
            command: FromOpenApiCommands::Server(GenerateArgs {
                input: Some(input_file.clone()),
                input_dir: None,
                output_dir: Some(dir.path().join("out3")),
                no_github_actions: false,
                no_installable_package: false,
            }),
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
        };
        let result = run_generation(&args, &cdd_core::strategies::ActixStrategy);
        assert!(result.is_err());
    }
}
