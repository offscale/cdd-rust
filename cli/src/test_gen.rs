#![deny(missing_docs)]

//! # Test Generator Command
//!
//! Generates the integration test file.

use std::fs;
use std::path::PathBuf;

use cdd_core::contract_test_generator::generate_contract_tests_file;
use cdd_core::oas::parse_openapi_routes;
use cdd_core::{AppError, AppResult};

/// Arguments for the test generation command.
#[derive(clap::Args, Debug, Clone)]
pub struct TestGenArgs {
    /// Path to the OpenAPI spec.
    #[clap(long, default_value = "docs/openapi.yaml")]
    pub openapi_path: PathBuf,

    /// Output path for the test file.
    #[clap(long, default_value = "tests/api_contracts.rs")]
    pub output_path: PathBuf,

    /// The function that initializes the Actix App (e.g. `web::create_app`).
    /// The generated code calls it as: `test::init_service({factory}(App::new()))`.
    #[clap(long, default_value = "crate::http::routes::config")]
    pub app_factory: String,
}

/// Executes the test generation.
pub fn execute(args: &TestGenArgs) -> AppResult<()> {
    if !args.openapi_path.exists() {
        return Err(AppError::General(format!(
            "OpenAPI file not found: {:?}",
            args.openapi_path
        )));
    }

    // 1. Read Schema
    let yaml_content = fs::read_to_string(&args.openapi_path)
        .map_err(|e| AppError::General(format!("Failed to read OpenAPI: {}", e)))?;

    // 2. Parse Routes
    let routes = parse_openapi_routes(&yaml_content)?;

    // 3. Generate Code
    // Note: The generated test needs the RELATIVE path to openapi.yaml from the project root at runtime,
    // or absolute. We pass the string provided in args, assuming user runs `cargo test` from root.
    let openapi_str = args.openapi_path.to_string_lossy().to_string();
    let code = generate_contract_tests_file(&routes, &openapi_str, &args.app_factory)?;

    // 4. Write File
    if let Some(parent) = args.output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::General(format!("Failed to create output dir: {}", e)))?;
    }

    fs::write(&args.output_path, code)
        .map_err(|e| AppError::General(format!("Failed to write test file: {}", e)))?;

    println!("Generated integration tests at {:?}", args.output_path);

    Ok(())
}
