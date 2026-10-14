#![deny(missing_docs)]

//! OpenAPI Generation Module

use crate::TargetMode;
use cdd_core::classes::parse::{extract_model, extract_struct_names};
use cdd_core::error::{AppError, AppResult};
use cdd_core::openapi::emit::{generate_openapi_document_with_routes_and_components, OpenApiInfo};
use cdd_core::openapi::parse::models::ParsedRoute;
use cdd_core::routes::parse::{
    parse_actix_routes, parse_axum_routes, parse_clap_cli_routes, parse_mcp_routes,
    parse_reqwest_routes,
};
use clap::Args;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Arguments for the `to_openapi` command.
#[derive(Args, Debug)]
pub struct ToOpenApiArgs {
    /// Input Rust project path or source file.
    #[clap(short = 'i', long, env = "CDD_INPUT")]
    pub input: PathBuf,

    /// Output file path for the generated OpenAPI spec.
    #[clap(short = 'o', long, default_value = "openapi.json", env = "CDD_OUTPUT")]
    pub output: PathBuf,

    /// Optional OpenAPI title override (defaults to `Cargo.toml` package name or `"Generated API"`).
    #[clap(long, env = "CDD_API_TITLE")]
    pub title: Option<String>,

    /// Optional OpenAPI version override (defaults to `Cargo.toml` package version or `"0.1.0"`).
    #[clap(long, env = "CDD_API_VERSION")]
    pub version: Option<String>,
}

/// Extracts `(name, version)` package metadata from a `Cargo.toml` file in `input` or its ancestor directory.
fn resolve_package_metadata(
    input: &Path,
    title_override: Option<&str>,
    version_override: Option<&str>,
) -> (String, String) {
    let mut parsed_name = None;
    let mut parsed_version = None;

    let search_dir = if input.is_file() {
        input.parent().unwrap_or(input)
    } else {
        input
    };

    let candidates = [
        search_dir.join("Cargo.toml"),
        search_dir
            .parent()
            .map(|p| p.join("Cargo.toml"))
            .unwrap_or_else(|| search_dir.join("Cargo.toml")),
    ];

    for candidate in &candidates {
        if candidate.is_file() {
            if let Ok(content) = fs::read_to_string(candidate) {
                let (n, v) = parse_cargo_toml_metadata(&content);
                if parsed_name.is_none() {
                    parsed_name = n;
                }
                if parsed_version.is_none() {
                    parsed_version = v;
                }
                if parsed_name.is_some() && parsed_version.is_some() {
                    break;
                }
            }
        }
    }

    let final_title = title_override
        .map(ToOwned::to_owned)
        .or(parsed_name)
        .unwrap_or_else(|| "Generated API".to_string());
    let final_version = version_override
        .map(ToOwned::to_owned)
        .or(parsed_version)
        .unwrap_or_else(|| "0.1.0".to_string());

    (final_title, final_version)
}

/// Parses `name` and `version` under `[package]` from `Cargo.toml` content.
fn parse_cargo_toml_metadata(content: &str) -> (Option<String>, Option<String>) {
    let mut in_package = false;
    let mut name = None;
    let mut version = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim();
            let val = v.trim().trim_matches('"').trim_matches(char::from(39u8));
            if key == "name" && !val.is_empty() {
                name = Some(val.to_string());
            } else if key == "version" && !val.is_empty() && !val.starts_with('{') {
                version = Some(val.to_string());
            }
        }
    }

    (name, version)
}

/// Dispatches route extraction based on [`TargetMode`], falling back to scanning all
/// supported route formats if the primary target parser returns no routes for a file.
fn parse_routes_for_target(code: &str, target: &TargetMode) -> AppResult<Vec<ParsedRoute>> {
    let mut routes = match target {
        TargetMode::ServerActix => parse_actix_routes(code)?,
        TargetMode::ServerAxum => parse_axum_routes(code)?,
        TargetMode::ClientReqwest => parse_reqwest_routes(code)?,
        TargetMode::ClientInternal => parse_clap_cli_routes(code)?,
    };

    if routes.is_empty() {
        if let Ok(req_routes) = parse_reqwest_routes(code) {
            routes.extend(req_routes);
        }
    }
    if routes.is_empty() {
        if let Ok(actix_routes) = parse_actix_routes(code) {
            routes.extend(actix_routes);
        }
    }
    if routes.is_empty() {
        if let Ok(mcp_routes) = parse_mcp_routes(code) {
            routes.extend(mcp_routes);
        }
    }

    Ok(routes)
}

/// Execute the `to_openapi` command, scanning Rust code and emitting an OpenAPI specification.
///
/// # Arguments
/// * `args` - The command-line arguments for `to_openapi`.
/// * `target` - The target mode determining the primary route parser strategy.
///
/// # Returns
/// `Ok(())` when the OpenAPI specification has been written to `args.output`.
///
/// # Errors
/// Returns [`AppError`] if the input path does not exist or serialization/IO fails.
pub fn run_to_openapi(args: &ToOpenApiArgs, target: &TargetMode) -> AppResult<()> {
    println!("Generating OpenAPI specification from {:?}...", args.input);

    if !args.input.exists() {
        return Err(AppError::General(format!(
            "Input path {:?} does not exist.",
            args.input
        )));
    }

    let mut rust_files = Vec::new();
    if args.input.is_file() {
        rust_files.push(args.input.clone());
    } else {
        for entry in WalkDir::new(&args.input).into_iter().flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "rs" {
                        rust_files.push(path.to_path_buf());
                    }
                }
            }
        }
    }

    let (title, version) =
        resolve_package_metadata(&args.input, args.title.as_deref(), args.version.as_deref());
    let mut parsed_models = Vec::new();
    let mut parsed_routes = Vec::new();

    for file in rust_files {
        if let Ok(code) = fs::read_to_string(&file) {
            // 1. Try to extract structs (models)
            if let Ok(struct_names) = extract_struct_names(&code) {
                for name in struct_names {
                    if let Ok(model) = extract_model(&code, &name) {
                        parsed_models.push(model);
                    }
                }
            }

            // 2. Extract routes using target-aware parser
            if let Ok(routes) = parse_routes_for_target(&code, target) {
                parsed_routes.extend(routes);
            }
        }
    }

    let info = OpenApiInfo::new(title, version);
    let doc = generate_openapi_document_with_routes_and_components(
        &parsed_models,
        &parsed_routes,
        None,
        &info,
        None,
    )?;

    let ext = args
        .output
        .extension()
        .unwrap_or_default()
        .to_string_lossy();
    let content = serialize_doc(&doc, &ext)?;

    if let Some(parent) = args.output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| AppError::General(e.to_string()))?;
        }
    }

    fs::write(&args.output, content).map_err(|e| AppError::General(e.to_string()))?;
    println!(
        "Successfully wrote OpenAPI specification to {:?}",
        args.output
    );

    Ok(())
}

/// Serializes the OpenAPI document to JSON or YAML depending on extension.
fn serialize_doc(doc: &serde_json::Value, ext: &str) -> AppResult<String> {
    if ext == "json" {
        Ok(serde_json::to_string_pretty(doc)?)
    } else {
        Ok(serde_yaml::to_string(doc)?)
    }
}

/// Configuration for `to_openapi` programmatic API.
#[derive(Debug, Default, Clone)]
pub struct ToOpenApiConfig {
    /// Input path.
    pub input: PathBuf,
    /// Output path.
    pub output: PathBuf,
    /// Target mode for route extraction.
    pub target: TargetMode,
    /// Optional API title override.
    pub title: Option<String>,
    /// Optional API version override.
    pub version: Option<String>,
}

/// Generate an OpenAPI specification from a Rust project.
///
/// # Arguments
/// * `config` - Programmatic configuration for `to_openapi`.
///
/// # Returns
/// `Ok(())` on success.
///
/// # Errors
/// Returns [`AppError`] if extraction or writing fails.
pub fn to_openapi(config: &ToOpenApiConfig) -> AppResult<()> {
    let args = ToOpenApiArgs {
        input: config.input.clone(),
        output: config.output.clone(),
        title: config.title.clone(),
        version: config.version.clone(),
    };
    run_to_openapi(&args, &config.target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_to_openapi_nonexistent() {
        let args = ToOpenApiArgs {
            input: PathBuf::from("/does/not/exist"),
            output: PathBuf::from("out.json"),
            title: None,
            version: None,
        };
        let res = run_to_openapi(&args, &TargetMode::ServerActix);
        assert!(res.is_err());
    }

    #[test]
    fn test_to_openapi_directory_and_file() {
        let dir = tempdir().expect("Failed to create temporary directory");
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).expect("Failed to create directory");

        let cargo_toml = dir.path().join("Cargo.toml");
        fs::write(
            &cargo_toml,
            r#"[package]
name = "my-petstore"
version = "2.3.4"
[dependencies]
serde = "1.0"
"#,
        )
        .expect("Failed to write Cargo.toml");

        let rust_file = src_dir.join("lib.rs");
        let code = r#"
use serde::{Serialize, Deserialize};
use actix_web::{get, post, web, HttpResponse, Responder};

/// A user model
#[derive(Serialize, Deserialize)]
pub struct User {
    pub id: i32,
    pub name: String,
}

/// Get user
#[get("/users/{id}")]
pub async fn get_user(id: web::Path<i32>) -> impl Responder {
    HttpResponse::Ok().finish()
}

/// Reqwest style
/// @OAS_METHOD: POST
/// @OAS_PATH: /users
pub async fn create_user() {}
"#;
        fs::write(&rust_file, code).expect("Failed to write to file");
        fs::write(src_dir.join("ignore.txt"), "hello").expect("Failed to write to file");

        // 1. Test directory input -> JSON output (picks up Cargo.toml metadata)
        let out_json = dir.path().join("out").join("openapi.json");
        let args_json = ToOpenApiArgs {
            input: dir.path().to_path_buf(),
            output: out_json.clone(),
            title: None,
            version: None,
        };
        assert!(run_to_openapi(&args_json, &TargetMode::ServerActix).is_ok());
        assert!(out_json.exists());
        let content_json = fs::read_to_string(&out_json).expect("Failed to read file to string");
        assert!(content_json.contains(r#""User""#));
        assert!(content_json.contains(r#""/users/{id}""#));
        assert!(content_json.contains(r#""my-petstore""#));
        assert!(content_json.contains(r#""2.3.4""#));

        // 2. Test file input -> YAML output with explicit title/version overrides
        let out_yaml = dir.path().join("openapi.yaml");
        let args_yaml = ToOpenApiArgs {
            input: rust_file,
            output: out_yaml.clone(),
            title: Some("Custom API".to_string()),
            version: Some("9.9.9".to_string()),
        };
        assert!(run_to_openapi(&args_yaml, &TargetMode::ClientReqwest).is_ok());
        assert!(out_yaml.exists());
        let content_yaml = fs::read_to_string(&out_yaml).expect("Failed to read file to string");
        assert!(content_yaml.contains("User:"));
        assert!(content_yaml.contains("/users:"));
        assert!(content_yaml.contains("Custom API"));
        assert!(content_yaml.contains("9.9.9"));
    }

    #[test]
    fn test_target_modes_axum_cli_mcp() {
        let axum_code = r#"
        pub fn app() -> axum::Router {
            axum::Router::new().route("/axum/:id", axum::routing::get(get_axum))
        }
        "#;
        let axum_routes =
            parse_routes_for_target(axum_code, &TargetMode::ServerAxum).expect("axum routes");
        assert_eq!(axum_routes.len(), 1);
        assert_eq!(axum_routes[0].path, "/axum/{id}");

        let cli_code = r#"
        /// @OAS_METHOD: GET
        /// @OAS_PATH: /cli
        pub async fn cli_cmd() {}
        "#;
        let cli_routes =
            parse_routes_for_target(cli_code, &TargetMode::ClientInternal).expect("cli routes");
        assert_eq!(cli_routes.len(), 1);
        assert_eq!(cli_routes[0].path, "/cli");

        let mcp_code = r#"
        /// @MCP_TOOL: mcp_op
        pub async fn mcp_op() {}
        "#;
        let mcp_routes =
            parse_routes_for_target(mcp_code, &TargetMode::ServerActix).expect("mcp routes");
        assert_eq!(mcp_routes.len(), 1);
        assert_eq!(mcp_routes[0].path, "/mcp_op");
    }

    #[test]
    fn test_generate_to_openapi_programmatic() {
        let dir = tempdir().expect("Failed to create temporary directory");
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("Failed to create directory");
        let rust_file = src_dir.join("lib.rs");
        std::fs::write(&rust_file, "pub struct Empty {}").expect("Failed to write to file");

        let out_json = dir.path().join("openapi.json");
        let config = ToOpenApiConfig {
            input: dir.path().to_path_buf(),
            output: out_json.clone(),
            target: TargetMode::ServerActix,
            title: None,
            version: None,
        };
        assert!(to_openapi(&config).is_ok());
        assert!(out_json.exists());
    }
}
