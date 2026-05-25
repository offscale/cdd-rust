//! OpenAPI Generation Module
use crate::TargetMode;
use cdd_core::classes::parse::{extract_model, extract_struct_names};
use cdd_core::error::{AppError, AppResult};
use cdd_core::openapi::emit::{generate_openapi_document_with_routes_and_components, OpenApiInfo};
use cdd_core::routes::parse::{parse_actix_routes, parse_reqwest_routes};
use clap::Args;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

/// Arguments for generating an OpenAPI spec from source code.
#[derive(Args, Debug)]
pub struct ToOpenApiArgs {
    /// Path to the code directory or file to parse.
    #[clap(short = 'i', long, env = "CDD_INPUT")]
    pub input: PathBuf,

    /// Output file for the generated OpenAPI spec.
    #[clap(short = 'o', long, env = "CDD_OUTPUT", default_value = "spec.json")]
    pub output: PathBuf,
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn serialize_doc(doc: &serde_json::Value, is_json: bool) -> AppResult<String> {
    if is_json {
        match serde_json::to_string_pretty(doc) {
            Ok(s) => Ok(s),
            Err(e) => Err(AppError::General(format!("Serialization failed: {}", e))),
        }
    } else {
        match serde_yaml::to_string(doc) {
            Ok(s) => Ok(s),
            Err(e) => Err(AppError::General(format!("Serialization failed: {}", e))),
        }
    }
}

/// Executes the OpenAPI generation from source code.
pub fn execute(args: &ToOpenApiArgs, _target: &TargetMode) -> AppResult<()> {
    println!("Extracting OpenAPI specification from {:?}", args.input);

    if !args.input.exists() {
        return Err(AppError::General(format!(
            "Path not found: {:?}",
            args.input
        )));
    }

    let mut parsed_models = Vec::new();
    let mut parsed_routes = Vec::new();

    // Walk directory and parse models and routes
    let walker = WalkDir::new(&args.input);
    for entry in walker.into_iter().flatten() {
        let path = entry.path();
        if path.extension() == Some(std::ffi::OsStr::new("rs")) {
            if let Ok(content) = fs::read_to_string(path) {
                // Parse models
                let struct_names = extract_struct_names(&content).unwrap_or_default();
                for name in struct_names {
                    if let Ok(model) = extract_model(&content, &name) {
                        parsed_models.push(model);
                    }
                }

                // Parse routes
                if let Ok(routes) = parse_actix_routes(&content) {
                    parsed_routes.extend(routes);
                }
                if let Ok(routes) = parse_reqwest_routes(&content) {
                    parsed_routes.extend(routes);
                }
            }
        }
    }

    let info = OpenApiInfo::new("Generated API", "1.0.0");
    let doc = generate_openapi_document_with_routes_and_components(
        &parsed_models,
        &parsed_routes,
        None,
        &info,
        None,
    )?;

    let is_json = args.output.extension() == Some(std::ffi::OsStr::new("json"));
    let output_str = serialize_doc(&doc, is_json)?;

    if let Err(e) = fs::write(&args.output, output_str) {
        return Err(AppError::General(format!(
            "Failed to write to file {:?}: {}",
            args.output, e
        )));
    }

    println!("OpenAPI spec successfully written to {:?}", args.output);

    Ok(())
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_to_openapi_execute_dir_read_error() {
        let args = ToOpenApiArgs {
            input: std::path::PathBuf::from("/does/not/exist"),
            output: std::path::PathBuf::from("out"),
        };
        assert!(execute(&args, &TargetMode::Cli).is_err());
    }

    #[test]
    fn test_to_openapi_execute() {
        let dir = tempdir().expect("Failed to create temporary directory");
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).expect("Failed to create");

        let rs_path = src_dir.join("models.rs");
        let rs_code = r#"
        pub struct User {
            pub id: i32,
            pub name: String,
        }
        
        #[get("/users")]
        pub async fn get_users() {}
        "#;
        File::create(&rs_path)
            .expect("Failed to create")
            .write_all(rs_code.as_bytes())
            .expect("Failed to write to file");

        let args = ToOpenApiArgs {
            input: src_dir,
            output: dir.path().join("spec.json"),
        };

        let result = execute(&args, &TargetMode::ServerActix);
        assert!(result.is_ok());
    }

    #[test]
    fn test_to_openapi_file_not_found() {
        let args = ToOpenApiArgs {
            input: PathBuf::from("does_not_exist_dir"),
            output: PathBuf::from("spec.json"),
        };
        let result = execute(&args, &TargetMode::ServerActix);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_openapi_client_and_cli_targets() {
        let dir = tempdir().expect("Failed to create temporary directory");
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("Failed to create");

        let rs_path = src_dir.join("clients.rs");
        let rs_code = r#"
        /// @OAS_METHOD: GET
        /// @OAS_PATH: /users
        pub async fn get_users(client: &reqwest::Client, base_url: &str) -> Result<reqwest::Response, reqwest::Error> {
            todo!()
        }
        "#;
        File::create(&rs_path)
            .expect("Failed to create")
            .write_all(rs_code.as_bytes())
            .expect("Failed to write to file");

        let args = ToOpenApiArgs {
            input: src_dir.clone(),
            output: dir.path().join("spec.json"),
        };

        let result_client = execute(&args, &TargetMode::Client);
        assert!(result_client.is_ok());

        let result_cli = execute(&args, &TargetMode::Cli);
        assert!(result_cli.is_ok());
    }

    #[test]
    fn test_to_openapi_execute_failures() {
        let dir = tempdir().expect("Failed to create temporary directory");
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let out_file = dir.path().join("out.yaml");

        // 1. Directory named .rs so read_to_string fails
        let dir_rs = src_dir.join("dir.rs");
        std::fs::create_dir(&dir_rs).unwrap();

        // 2. File where extract_struct_names fails
        let syntax_err_rs = src_dir.join("syntax.rs");
        std::fs::write(&syntax_err_rs, "impl Model {").unwrap();

        // 3. File where extract_model fails
        let model_err_rs = src_dir.join("model_err.rs");
        std::fs::write(&model_err_rs, "pub struct Partial").unwrap();

        // 4. File where parse routes fail
        let route_err_rs = src_dir.join("route_err.rs");
        std::fs::write(&route_err_rs, "pub fn my_route() {}").unwrap();

        let args = ToOpenApiArgs {
            input: src_dir,
            output: out_file.clone(),
        };

        // Execute should succeed but skip the bad files
        let _ = execute(&args, &TargetMode::ServerActix);
        let _ = execute(&args, &TargetMode::Client);
        let _ = execute(&args, &TargetMode::Cli);
    }

    #[test]
    fn test_to_openapi_execute_invalid_syntax() {
        let dir = tempdir().expect("Failed to create temporary directory");
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).expect("Failed to create");

        let rs_path = src_dir.join("invalid.rs");
        let rs_code = r#"
        pub struct { INVALID SYNTAX
        "#;
        File::create(&rs_path)
            .expect("Failed to create")
            .write_all(rs_code.as_bytes())
            .expect("Failed to write to file");

        let args = ToOpenApiArgs {
            input: src_dir,
            output: dir.path().join("spec.json"),
        };

        let result = execute(&args, &TargetMode::Cli);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_openapi_execute_yaml() {
        use tempfile::tempdir;
        let dir = tempdir().expect("Failed to create temporary directory");
        let input_file = dir.path().join("input.rs");
        let output_file = dir.path().join("output.yaml");

        let handler_code = r#"
        #[openapi]
        #[get("/test")]
        async fn test_handler() -> impl Responder { HttpResponse::Ok().finish() }
        "#;
        std::fs::write(&input_file, handler_code).expect("Failed to write to file");

        let args = ToOpenApiArgs {
            input: input_file,
            output: output_file.clone(),
        };

        let result = execute(&args, &crate::TargetMode::ServerActix);
        assert!(result.is_ok());
        assert!(std::fs::read_to_string(output_file)
            .expect("Failed to read file to string")
            .contains("openapi:"));
    }

    #[test]
    fn test_to_openapi_execute_write_error() {
        use tempfile::tempdir;
        let dir = tempdir().expect("Failed to create temporary directory");
        let input_file = dir.path().join("input.rs");

        let handler_code = r#"
        #[openapi]
        #[get("/test")]
        async fn test_handler() -> impl Responder { HttpResponse::Ok().finish() }
        "#;
        std::fs::write(&input_file, handler_code).expect("Failed to write to file");

        let args = ToOpenApiArgs {
            input: input_file,
            output: std::path::PathBuf::from("/nonexistent_dir/output.yaml"),
        };

        let result = execute(&args, &crate::TargetMode::ServerActix);
        assert!(result.is_err());
    }
}
