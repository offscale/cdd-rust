#![deny(missing_docs)]

//! Module for generating scaffolding from OpenAPI specifications.

use cdd_core::error::{AppError, AppResult};
use cdd_core::strategies::{
    ActixStrategy, AxumStrategy, BackendStrategy, ClapCliStrategy, CliStrategy, McpClientStrategy,
    McpServerStrategy, ReqwestStrategy, StrategyKind,
};
use clap::{Args, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::scaffold::ScaffoldArgs;
use crate::test_gen::TestGenArgs;

/// Available server frameworks.
#[derive(clap::ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ServerFramework {
    /// Actix Web framework.
    #[default]
    ActixWeb,
    /// Axum framework.
    Axum,
}

/// Strongly typed subcommand for `from_openapi` programmatic configuration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FromOpenApiSubcommand {
    /// Generate a Reqwest Client SDK (`to_sdk`).
    #[default]
    ToSdk,
    /// Generate a Client SDK and Clap CLI binary (`to_sdk_cli`).
    ToSdkCli,
    /// Generate an HTTP Server (`to_server`).
    ToServer,
}

impl FromStr for FromOpenApiSubcommand {
    type Err = AppError;

    /// Parses a string subcommand name into `FromOpenApiSubcommand`.
    ///
    /// # Arguments
    /// * `s` - The string identifier (`"to_sdk"`, `"to_sdk_cli"`, or `"to_server"`).
    ///
    /// # Returns
    /// The parsed [`FromOpenApiSubcommand`].
    ///
    /// # Errors
    /// Returns [`AppError::General`] if the subcommand name is unrecognized.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "to_sdk" => Ok(Self::ToSdk),
            "to_sdk_cli" => Ok(Self::ToSdkCli),
            "to_server" => Ok(Self::ToServer),
            _ => Err(AppError::General(format!("Unknown subcommand: {}", s))),
        }
    }
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
    /// Generate an offline-first Clap CLI SDK.
    #[clap(name = "to_sdk_cli")]
    SdkCli(GenerateArgs),
    /// Generate a Client SDK.
    #[clap(name = "to_sdk")]
    Sdk(GenerateArgs),
    /// Generate Server scaffolding.
    #[clap(name = "to_server")]
    Server {
        /// The common arguments for generation.
        #[clap(flatten)]
        args: GenerateArgs,
        /// The target server framework (actix-web or axum). Defaults to actix-web.
        #[clap(long, default_value = "actix-web", env = "CDD_SERVER_FRAMEWORK")]
        framework: ServerFramework,
    },
}

/// Common arguments used for generation commands.
#[derive(Args, Debug, Clone)]
pub struct GenerateArgs {
    /// Path or URL to the OpenAPI specification.
    #[clap(
        short = 'i',
        long,
        required_unless_present = "input_dir",
        env = "CDD_INPUT"
    )]
    pub input: Option<PathBuf>,

    /// Directory containing OpenAPI specifications.
    #[clap(
        short = 'd',
        long,
        required_unless_present = "input",
        env = "CDD_INPUT_DIR"
    )]
    pub input_dir: Option<PathBuf>,

    /// Output file or directory path.
    #[clap(short = 'o', long = "output", env = "CDD_OUTPUT")]
    pub output_dir: Option<PathBuf>,

    /// Do not generate GitHub Actions scaffolding.
    #[clap(long, env = "CDD_NO_GITHUB_ACTIONS")]
    pub no_github_actions: bool,

    /// Do not generate installable package scaffolding.
    #[clap(long, env = "CDD_NO_INSTALLABLE_PACKAGE")]
    pub no_installable_package: bool,

    /// Generate integration tests and mocks.
    #[clap(long, env = "CDD_TESTS")]
    pub tests: bool,

    /// Generate Model Context Protocol (MCP) server and adapter.
    #[clap(short = 'm', long, env = "CDD_MCP")]
    pub mcp: bool,
}

impl GenerateArgs {
    /// Get the output directory, defaulting to the current directory.
    ///
    /// # Returns
    /// The resolved output [`PathBuf`].
    pub fn get_output_dir(&self) -> PathBuf {
        self.output_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Retrieve the input files either from the single file or the directory specified.
    ///
    /// # Returns
    /// A list of OpenAPI specification [`PathBuf`]s.
    pub fn get_input_files(&self) -> Vec<PathBuf> {
        if let Some(ref file) = self.input {
            vec![file.clone()]
        } else if let Some(ref dir) = self.input_dir {
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

/// Checks if a string represents an HTTP or HTTPS URL using `url::Url::parse`.
///
/// # Arguments
/// * `input` - The input string to inspect.
///
/// # Returns
/// `true` if `input` is a valid `http` or `https` URL.
pub fn is_http_url(input: &str) -> bool {
    url::Url::parse(input)
        .map(|u| u.scheme() == "http" || u.scheme() == "https")
        .unwrap_or(false)
}

/// Reads an OpenAPI specification from a local path or HTTP(S) URL.
///
/// # Arguments
/// * `input` - Path or URL to the OpenAPI specification.
///
/// # Returns
/// The raw specification content as a [`String`].
///
/// # Errors
/// Returns [`AppError`] if the file or URL cannot be read.
#[cfg(all(feature = "client", not(target_os = "wasi")))]
pub fn read_spec_source(input: &Path) -> AppResult<String> {
    let input_str = input.to_string_lossy();
    if is_http_url(&input_str) {
        let mut response = ureq::get(input_str.as_ref())
            .call()
            .map_err(|e| AppError::General(format!("Failed to fetch URL: {}", e)))?;
        let mut body = String::new();
        std::io::Read::read_to_string(&mut response.body_mut().as_reader(), &mut body)?;
        Ok(body)
    } else {
        Ok(fs::read_to_string(input)?)
    }
}

/// Reads an OpenAPI specification from a local path only (when `client` feature is disabled).
///
/// # Arguments
/// * `input` - Path to the OpenAPI specification.
///
/// # Returns
/// The raw specification content as a [`String`].
///
/// # Errors
/// Returns [`AppError`] if `input` is an HTTP URL or if the file cannot be read.
#[cfg(any(not(feature = "client"), target_os = "wasi"))]
pub fn read_spec_source(input: &Path) -> AppResult<String> {
    let input_str = input.to_string_lossy();
    if is_http_url(&input_str) {
        Err(AppError::General(
            "Client feature is not compiled or not supported on this platform, cannot fetch HTTP URLs".to_string(),
        ))
    } else {
        fs::read_to_string(input)
            .map_err(|e| AppError::General(format!("Failed to read OpenAPI {:?}: {}", input, e)))
    }
}

/// Executes the requested OpenAPI generation command.
///
/// # Arguments
/// * `args` - Parsed CLI arguments for `from_openapi`.
///
/// # Returns
/// `Ok(())` on successful code generation.
///
/// # Errors
/// Returns [`AppError`] if inputs are missing or file generation fails.
pub fn run_from_openapi(args: &FromOpenApiArgs) -> AppResult<()> {
    match &args.command {
        FromOpenApiCommands::SdkCli(gen_args) => {
            println!("Generating SDK + CLI...");
            generate_sdk_cli(gen_args)
        }
        FromOpenApiCommands::Sdk(gen_args) => {
            println!("Generating SDK...");
            generate_sdk(gen_args)
        }
        FromOpenApiCommands::Server {
            args: gen_args,
            framework,
        } => {
            println!("Generating Server with framework {:?}...", framework);
            generate_server(gen_args, *framework)
        }
    }
}

/// Generates a client SDK (`ReqwestStrategy`) and optional `mcp` module (`McpClientStrategy`).
///
/// # Arguments
/// * `args` - Generation parameters.
///
/// # Returns
/// `Ok(())` on success.
///
/// # Errors
/// Returns [`AppError`] if reading specifications or writing files fails.
pub fn generate_sdk(args: &GenerateArgs) -> AppResult<()> {
    run_generation_with_subdir(args, &ReqwestStrategy, "handlers")?;
    if args.mcp {
        append_mcp_module(args, &McpClientStrategy)?;
    }
    Ok(())
}

/// Generates a cohesive SDK + CLI package where the Reqwest client SDK is generated
/// under `src/client/` and the Clap CLI handlers and binary entrypoint are generated
/// under `src/cli/` and `src/main.rs` without overwriting each other.
///
/// # Arguments
/// * `args` - Generation parameters.
///
/// # Returns
/// `Ok(())` on success.
///
/// # Errors
/// Returns [`AppError`] if reading specifications or writing files fails.
pub fn generate_sdk_cli(args: &GenerateArgs) -> AppResult<()> {
    run_generation_with_subdir(args, &ReqwestStrategy, "client").and_then(|()| {
        run_generation_with_subdir(args, &ClapCliStrategy, "handlers")?;
        if args.mcp {
            append_mcp_module(args, &McpClientStrategy)?;
        }
        Ok(())
    })
}

/// Generates server scaffolding (`ActixStrategy` or `AxumStrategy`) and optional `src/mcp/`
/// module (`McpServerStrategy`) without overwriting server handlers.
///
/// # Arguments
/// * `args` - Generation parameters.
/// * `framework` - Selected HTTP server framework.
///
/// # Returns
/// `Ok(())` on success.
///
/// # Errors
/// Returns [`AppError`] if reading specifications or writing files fails.
pub fn generate_server(args: &GenerateArgs, framework: ServerFramework) -> AppResult<()> {
    match framework {
        ServerFramework::ActixWeb => run_generation_with_subdir(args, &ActixStrategy, "handlers")?,
        ServerFramework::Axum => run_generation_with_subdir(args, &AxumStrategy, "handlers")?,
    }
    if args.mcp {
        append_mcp_module(args, &McpServerStrategy)?;
    }
    Ok(())
}

/// Appends an `src/mcp/` module and registers `pub mod mcp;` in `src/lib.rs` without
/// overwriting existing `src/handlers/` or `src/client/` modules.
fn append_mcp_module(args: &GenerateArgs, strategy: &impl BackendStrategy) -> AppResult<()> {
    let inputs = args.get_input_files();
    let out_dir = args.get_output_dir();
    let src_dir = out_dir.join("src");
    let mcp_dir = src_dir.join("mcp");
    fs::create_dir_all(&mcp_dir)?;

    for input in inputs {
        let scaffold_args = ScaffoldArgs {
            openapi_path: input,
            output_dir: mcp_dir.clone(),
            route_config_path: None,
            force: false,
        };
        crate::scaffold::run_scaffold(&scaffold_args, strategy)?;
    }

    write_mod_rs(&mcp_dir)?;
    ensure_lib_mod(&src_dir.join("lib.rs"), "pub mod mcp;\n")?;
    Ok(())
}

/// Ensures a module declaration line is present in `lib.rs`.
fn ensure_lib_mod(lib_rs_path: &Path, mod_decl: &str) -> AppResult<()> {
    let existing = if lib_rs_path.exists() {
        fs::read_to_string(lib_rs_path)?
    } else {
        String::new()
    };
    if !existing.contains(mod_decl.trim()) {
        let updated = format!("{}{}", mod_decl, existing);
        fs::write(lib_rs_path, updated)?;
    }
    Ok(())
}

/// Writes `mod.rs` listing all `.rs` sibling modules in `dir`.
fn write_mod_rs(dir: &Path) -> AppResult<Vec<String>> {
    let mut mods = Vec::new();
    let mut mod_names = Vec::new();
    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|name| name != "mod.rs")
            && path.extension().is_some_and(|ext| ext == "rs")
        {
            let mod_name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            mods.push(format!("pub mod {};\n", mod_name));
            mod_names.push(mod_name);
        }
    }
    mods.sort();
    mod_names.sort();
    fs::write(dir.join("mod.rs"), mods.join(""))?;
    Ok(mod_names)
}

/// Writes the `Cargo.toml` and `.github/workflows/ci.yml` skeleton files into `out_dir`.
pub(crate) fn write_package_skeleton_files(
    out_dir: &Path,
    no_installable_package: bool,
    no_github_actions: bool,
    dependencies: &str,
) -> AppResult<()> {
    fs::create_dir_all(out_dir)?;

    if !no_installable_package {
        let cargo_toml_path = out_dir.join("Cargo.toml");
        if !cargo_toml_path.exists() {
            let pkg_name = out_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "generated_package".to_string());

            let cargo_toml = format!(
                r#"[package]
name = "{}"
version = "0.0.1"
edition = "2021"

[dependencies]
{}
"#,
                pkg_name, dependencies
            );
            fs::write(cargo_toml_path, cargo_toml)?;
        }
    }

    if !no_github_actions {
        let gh_workflows_dir = out_dir.join(".github").join("workflows");
        fs::create_dir_all(&gh_workflows_dir)?;
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
      - uses: actions/checkout@v6
      - name: Build
        run: cargo build --verbose
      - name: Run tests
        run: cargo test --verbose
"#;
            fs::write(ci_yml_path, ci_yml)?;
        }
    }

    Ok(())
}

fn prepare_package_skeleton(
    args: &GenerateArgs,
    out_dir: &Path,
    extra_deps: &str,
) -> AppResult<()> {
    let mut dependencies = String::from(
        r#"tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_qs = "1.0"
reqwest = { version = "0.11", features = ["json", "multipart"] }
actix-web = "4.0"
clap = { version = "4.0", features = ["derive"] }
chrono = { version = "0.4", features = ["serde"] }
utoipa = "4.2"
uuid = { version = "1.8", features = ["serde", "v4"] }
"#,
    );
    dependencies.push_str(extra_deps);
    write_package_skeleton_files(
        out_dir,
        args.no_installable_package,
        args.no_github_actions,
        &dependencies,
    )
}

fn prepare_models_and_handlers_dir(
    out_dir: &Path,
    input: &Path,
    module_subdir: &str,
    strategy: &impl BackendStrategy,
) -> AppResult<(PathBuf, PathBuf, PathBuf)> {
    let is_http_server = strategy.kind() == StrategyKind::HttpServer;
    let yaml_content = read_spec_source(input)?;

    let src_dir = out_dir.join("src");
    fs::create_dir_all(&src_dir)?;
    let lib_rs_path = src_dir.join("lib.rs");

    let models_dir = src_dir.join("models");
    fs::create_dir_all(&models_dir)?;

    let models = cdd_core::openapi::parse::parse_openapi_spec(&yaml_content)?;
    if !models.is_empty() {
        let file_path = models_dir.join("generated.rs");
        let existing_models = fs::read_to_string(&file_path).unwrap_or_default();
        let rust_code = cdd_core::classes::emit::update_model_file(&existing_models, &models)?;
        fs::write(&file_path, rust_code)?;
        let mod_rs = "pub mod generated;\npub use generated::*;\n";
        fs::write(models_dir.join("mod.rs"), mod_rs)?;
        ensure_lib_mod(&lib_rs_path, "pub mod models;\n")?;
    }

    let handlers_dir = src_dir.join(module_subdir);
    fs::create_dir_all(&handlers_dir)?;
    ensure_lib_mod(&lib_rs_path, &format!("pub mod {};\n", module_subdir))?;

    if is_http_server {
        let current_lib = fs::read_to_string(&lib_rs_path).unwrap_or_default();
        if !current_lib.contains("pub fn config(") {
            let mut updated = current_lib;
            if !updated.contains("pub mod security;") {
                updated.push_str("pub mod security;\n\n");
            }
            updated.push_str(
                &strategy
                    .route_registration_skeleton()
                    .replace("use crate::http::handlers;\n", ""),
            );
            fs::write(&lib_rs_path, updated)?;
        }

        let security_mod_content = r#"// Security module stub
use std::marker::PhantomData;

#[derive(Clone, Default)]
pub struct ApiKey;

#[derive(Clone, Default)]
pub struct OAuth2<T>(pub PhantomData<T>);

#[derive(Clone, Default)]
pub struct Oidc;

#[derive(Clone, Default)]
pub struct PetstoreAuth<T>(pub PhantomData<T>);

pub mod scopes {
    #[derive(Clone)]
    pub struct WritePets;
    #[derive(Clone)]
    pub struct ReadPets;
}
"#;
        fs::write(src_dir.join("security.rs"), security_mod_content)?;
    }

    Ok((src_dir, lib_rs_path, handlers_dir))
}

/// Runs the generation using the specified strategy and target module subdirectory name.
fn run_generation_with_subdir(
    args: &GenerateArgs,
    strategy: &impl BackendStrategy,
    module_subdir: &str,
) -> AppResult<()> {
    let inputs = args.get_input_files();
    if inputs.is_empty() {
        return Err(AppError::General(
            "No input files provided or found.".into(),
        ));
    }

    let out_dir = args.get_output_dir();
    prepare_package_skeleton(args, &out_dir, &strategy.extra_cargo_dependencies())?;

    let is_http_server = strategy.kind() == StrategyKind::HttpServer;
    let is_cli = strategy.kind() == StrategyKind::Cli;

    for input in inputs {
        let (src_dir, lib_rs_path, handlers_dir) =
            prepare_models_and_handlers_dir(&out_dir, &input, module_subdir, strategy)?;

        let route_config_path = is_http_server.then_some(lib_rs_path);

        let scaffold_args = ScaffoldArgs {
            openapi_path: input.clone(),
            output_dir: handlers_dir.clone(),
            route_config_path,
            force: false,
        };
        crate::scaffold::run_scaffold(&scaffold_args, strategy)?;

        let mod_names = write_mod_rs(&handlers_dir)?;
        maybe_write_cli_main(is_cli, &src_dir, module_subdir, &mod_names)?;
        maybe_generate_tests(args.tests, &out_dir, &input, strategy)?;
    }

    Ok(())
}

fn maybe_write_cli_main(
    is_cli: bool,
    src_dir: &Path,
    module_subdir: &str,
    mod_names: &[String],
) -> AppResult<()> {
    if is_cli {
        let main_rs = ClapCliStrategy.generate_cli_main(module_subdir, mod_names);
        fs::write(src_dir.join("main.rs"), main_rs)?;
    }
    Ok(())
}

fn maybe_generate_tests(
    enable_tests: bool,
    out_dir: &Path,
    input: &Path,
    strategy: &impl BackendStrategy,
) -> AppResult<()> {
    if enable_tests {
        let tests_dir = out_dir.join("tests");
        fs::create_dir_all(&tests_dir)?;
        let crate_name = out_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .replace('-', "_");
        let test_gen_args = TestGenArgs {
            openapi_path: input.to_path_buf(),
            output_path: tests_dir.join("api_contracts.rs"),
            app_factory: crate_name,
        };
        crate::test_gen::run_test_gen(&test_gen_args, strategy)?;
    }
    Ok(())
}

/// Configuration for `from_openapi` programmatic API.
#[derive(Debug, Default, Clone)]
pub struct FromOpenApiConfig {
    /// The parsed subcommand.
    pub subcommand: FromOpenApiSubcommand,
    /// The input file path.
    pub input: Option<PathBuf>,
    /// The input directory path.
    pub input_dir: Option<PathBuf>,
    /// The output directory path.
    pub output_dir: Option<PathBuf>,
    /// Whether to skip github actions generation.
    pub no_github_actions: bool,
    /// Whether to skip package generation.
    pub no_installable_package: bool,
    /// Whether to generate tests.
    pub tests: bool,
    /// Generate Model Context Protocol (MCP) server and adapter.
    pub mcp: bool,
    /// The server framework to use.
    pub framework: ServerFramework,
}

/// Generate code from an OpenAPI specification.
///
/// # Arguments
/// * `config` - Programmatic configuration for `from_openapi`.
///
/// # Returns
/// `Ok(())` on success.
///
/// # Errors
/// Returns [`AppError`] if generation fails.
pub fn from_openapi(config: &FromOpenApiConfig) -> AppResult<()> {
    let gen_args = GenerateArgs {
        input: config.input.clone(),
        input_dir: config.input_dir.clone(),
        output_dir: config.output_dir.clone(),
        no_github_actions: config.no_github_actions,
        no_installable_package: config.no_installable_package,
        tests: config.tests,
        mcp: config.mcp,
    };

    match config.subcommand {
        FromOpenApiSubcommand::ToSdkCli => generate_sdk_cli(&gen_args),
        FromOpenApiSubcommand::ToSdk => generate_sdk(&gen_args),
        FromOpenApiSubcommand::ToServer => generate_server(&gen_args, config.framework),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_subcommand_from_str() {
        assert_eq!(
            "to_sdk".parse::<FromOpenApiSubcommand>().expect("to_sdk"),
            FromOpenApiSubcommand::ToSdk
        );
        assert_eq!(
            "to_sdk_cli"
                .parse::<FromOpenApiSubcommand>()
                .expect("to_sdk_cli"),
            FromOpenApiSubcommand::ToSdkCli
        );
        assert_eq!(
            "to_server"
                .parse::<FromOpenApiSubcommand>()
                .expect("to_server"),
            FromOpenApiSubcommand::ToServer
        );
        assert!("unknown_cmd".parse::<FromOpenApiSubcommand>().is_err());
    }

    #[test]
    fn test_is_http_url_and_read_spec_source() {
        assert!(is_http_url("http://example.com/openapi.yaml"));
        assert!(is_http_url("https://example.com/openapi.json"));
        assert!(!is_http_url("ftp://example.com/openapi.yaml"));
        assert!(!is_http_url("./local/spec.yaml"));

        let err = read_spec_source(Path::new("http://127.0.0.1:1/nonexistent.yaml"));
        assert!(err.is_err());

        // Start a minimal local HTTP server to test successful HTTP fetch
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 512];
            let _ = std::io::Read::read(&mut stream, &mut buf);
            let resp =
                "HTTP/1.1 200 OK\r\nContent-Length: 14\r\nConnection: close\r\n\r\nopenapi: 3.0.0";
            let _ = std::io::Write::write_all(&mut stream, resp.as_bytes());
        });
        let url_str = format!("http://{}/openapi.yaml", addr);
        let body = read_spec_source(Path::new(&url_str)).expect("http fetch");
        assert_eq!(body, "openapi: 3.0.0");
        let _ = handle.join();
    }

    #[test]
    fn test_get_output_dir_default() {
        let args = GenerateArgs {
            input: None,
            input_dir: None,
            output_dir: None,
            no_github_actions: false,
            no_installable_package: false,
            tests: false,
            mcp: false,
        };
        let dir = args.get_output_dir();
        assert_eq!(dir, PathBuf::from("."));
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
            mcp: false,
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
            mcp: false,
        };
        assert_eq!(args.get_input_files(), vec![path]);
    }

    #[test]
    fn test_get_input_files_dir() {
        let dir = tempdir().expect("Failed to create temporary directory");
        let file1 = dir.path().join("a.yaml");
        let file2 = dir.path().join("b.json");
        let file3 = dir.path().join("c.txt");
        let file4 = dir.path().join("d.yml");
        let file_no_ext = dir.path().join("noext");
        let sub_dir = dir.path().join("subdir");
        fs::write(&file1, "").expect("Failed to write to file");
        fs::write(&file2, "").expect("Failed to write to file");
        fs::write(&file3, "").expect("Failed to write to file");
        fs::write(&file4, "").expect("Failed to write to file");
        fs::write(&file_no_ext, "").expect("Failed to write to file");
        fs::create_dir_all(&sub_dir).expect("mkdir");

        let args = GenerateArgs {
            input: None,
            input_dir: Some(dir.path().to_path_buf()),
            output_dir: None,
            no_github_actions: false,
            no_installable_package: false,
            tests: false,
            mcp: false,
        };
        let mut files = args.get_input_files();
        files.sort();
        let mut expected = vec![file1, file2, file4];
        expected.sort();
        assert_eq!(files, expected);

        let args_missing_dir = GenerateArgs {
            input_dir: Some(dir.path().join("does_not_exist")),
            ..args
        };
        assert!(args_missing_dir.get_input_files().is_empty());
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
            mcp: false,
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
      operationId: test__op
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

        // Test SdkCli with mcp - verifies both client/ and handlers/ and mcp/ coexist
        let out_sdk_cli_mcp = dir.path().join("out1_mcp");
        let args_mcp = FromOpenApiArgs {
            command: FromOpenApiCommands::SdkCli(GenerateArgs {
                input: Some(input_file.clone()),
                input_dir: None,
                output_dir: Some(out_sdk_cli_mcp.clone()),
                no_github_actions: false,
                no_installable_package: false,
                tests: false,
                mcp: true,
            }),
        };
        assert!(run_from_openapi(&args_mcp).is_ok());
        assert!(out_sdk_cli_mcp.join("src/client/mod.rs").exists());
        assert!(out_sdk_cli_mcp.join("src/handlers/mod.rs").exists());
        assert!(out_sdk_cli_mcp.join("src/mcp/mod.rs").exists());
        assert!(out_sdk_cli_mcp.join("src/main.rs").exists());

        // Test SdkCli without mcp
        let args_cli_no_mcp = FromOpenApiArgs {
            command: FromOpenApiCommands::SdkCli(GenerateArgs {
                input: Some(input_file.clone()),
                input_dir: None,
                output_dir: Some(dir.path().join("out1_no_mcp")),
                no_github_actions: true,
                no_installable_package: true,
                tests: true,
                mcp: false,
            }),
        };
        assert!(run_from_openapi(&args_cli_no_mcp).is_ok());

        // Test Sdk with mcp
        let out_sdk = dir.path().join("out2");
        let args = FromOpenApiArgs {
            command: FromOpenApiCommands::Sdk(GenerateArgs {
                input: Some(input_file.clone()),
                input_dir: None,
                output_dir: Some(out_sdk.clone()),
                no_github_actions: false,
                no_installable_package: false,
                tests: false,
                mcp: true,
            }),
        };
        assert!(run_from_openapi(&args).is_ok());
        assert!(out_sdk.join("src/handlers/mod.rs").exists());
        assert!(out_sdk.join("src/mcp/mod.rs").exists());

        // Test Server with ActixWeb and mcp - verifies handlers/ and mcp/ coexist
        let out_srv = dir.path().join("out3");
        let srv_lib = out_srv.join("src");
        fs::create_dir_all(&srv_lib).expect("mkdir");
        fs::write(srv_lib.join("lib.rs"), "pub mod security;\n").expect("write lib.rs");
        fs::write(srv_lib.join("extra.txt"), "ignore").expect("write txt");
        let args = FromOpenApiArgs {
            command: FromOpenApiCommands::Server {
                args: GenerateArgs {
                    input: Some(input_file.clone()),
                    input_dir: None,
                    output_dir: Some(out_srv.clone()),
                    no_github_actions: false,
                    no_installable_package: false,
                    tests: false,
                    mcp: true,
                },
                framework: ServerFramework::ActixWeb,
            },
        };
        assert!(run_from_openapi(&args).is_ok());
        // Run a second time when Cargo.toml, ci.yml, and pub fn config( already exist
        assert!(run_from_openapi(&args).is_ok());
        assert!(out_srv.join("src/handlers/mod.rs").exists());
        assert!(out_srv.join("src/mcp/mod.rs").exists());

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
                    mcp: false,
                },
                framework: ServerFramework::Axum,
            },
        };
        assert!(run_from_openapi(&args).is_ok());
    }

    #[test]
    fn test_run_generation_no_inputs() {
        let args = GenerateArgs {
            input: None,
            input_dir: None,
            output_dir: None,
            no_github_actions: false,
            no_installable_package: false,
            tests: false,
            mcp: false,
        };
        assert!(run_generation_with_subdir(&args, &ActixStrategy, "handlers").is_err());
        assert!(run_generation_with_subdir(&args, &AxumStrategy, "handlers").is_err());
        assert!(run_generation_with_subdir(&args, &ReqwestStrategy, "client").is_err());
        assert!(run_generation_with_subdir(&args, &ClapCliStrategy, "handlers").is_err());
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
            mcp: false,
        };
        let result = run_generation_with_subdir(&args, &ActixStrategy, "handlers");
        assert!(result.is_err());
    }

    #[test]
    fn test_run_generation_with_tests_flag() {
        let dir = tempdir().expect("tempdir");
        let input = dir.path().join("spec.json");
        std::fs::write(
            &input,
            r#"{"openapi": "3.0.0", "info": {"title": "Test", "version": "1.0"}, "paths": {}}"#,
        )
        .expect("write");
        let args = GenerateArgs {
            input: Some(input),
            input_dir: None,
            output_dir: Some(dir.path().join("my_out_app")),
            no_github_actions: true,
            no_installable_package: true,
            tests: true,
            mcp: false,
        };
        let res = run_generation_with_subdir(&args, &ActixStrategy, "handlers");
        assert!(res.is_ok());
        assert!(run_generation_with_subdir(&args, &AxumStrategy, "handlers").is_ok());
        assert!(run_generation_with_subdir(&args, &ReqwestStrategy, "client").is_ok());

        let mod_dir = dir.path().join("my_out_app/src/handlers");
        fs::write(mod_dir.join("ignore.txt"), "txt").expect("write txt");
        assert!(write_mod_rs(&mod_dir).is_ok());

        // Error propagation in generate_sdk, generate_sdk_cli, generate_server, append_mcp_module
        let bad_args = GenerateArgs {
            input: Some(dir.path().join("nonexistent.yaml")),
            input_dir: None,
            output_dir: Some(PathBuf::from("/")),
            no_github_actions: false,
            no_installable_package: false,
            tests: false,
            mcp: true,
        };
        assert!(generate_sdk(&bad_args).is_err());
        assert!(generate_sdk_cli(&bad_args).is_err());
        assert!(generate_server(&bad_args, ServerFramework::ActixWeb).is_err());
        assert!(generate_server(&bad_args, ServerFramework::Axum).is_err());
        assert!(append_mcp_module(&bad_args, &McpClientStrategy).is_err());
        assert!(append_mcp_module(&bad_args, &McpServerStrategy).is_err());
    }

    #[test]
    fn test_generate_from_openapi_programmatic() {
        let dir = tempdir().expect("Failed to create temporary directory");
        let input_file = dir.path().join("spec.yaml");
        let openapi_content = r#"openapi: 3.0.0
info:
  title: Test API
  version: 1.0.0
paths:
  /test:
    get:
      operationId: test__op
      responses:
        "200":
          description: OK"#;
        std::fs::write(&input_file, openapi_content).expect("Failed to write to file");

        let config_cli = FromOpenApiConfig {
            subcommand: FromOpenApiSubcommand::ToSdkCli,
            input: Some(input_file.clone()),
            output_dir: Some(dir.path().join("out_cli_mcp")),
            mcp: true,
            ..Default::default()
        };
        assert!(from_openapi(&config_cli).is_ok());

        let config_sdk = FromOpenApiConfig {
            subcommand: FromOpenApiSubcommand::ToSdk,
            input: Some(input_file.clone()),
            output_dir: Some(dir.path().join("out_sdk")),
            ..Default::default()
        };
        assert!(from_openapi(&config_sdk).is_ok());

        let config_srv = FromOpenApiConfig {
            subcommand: FromOpenApiSubcommand::ToServer,
            input: Some(input_file),
            output_dir: Some(dir.path().join("out_srv")),
            framework: ServerFramework::ActixWeb,
            ..Default::default()
        };
        assert!(from_openapi(&config_srv).is_ok());
    }
}
