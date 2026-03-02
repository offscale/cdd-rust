use crate::TargetMode;
use cdd_core::classes::parse::{extract_model, extract_struct_names};
use cdd_core::error::{AppError, AppResult};
use cdd_core::openapi::emit::{generate_openapi_document_with_routes_and_components, OpenApiInfo};
use cdd_core::routes::parse::{parse_actix_routes, parse_reqwest_routes};
use clap::Args;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Args, Debug)]
pub struct ToOpenApiArgs {
    /// Path to the code directory or file to parse.
    #[clap(short = 'f', long, env = "CDD_FILE")]
    pub file: PathBuf,

    /// Output file for the generated OpenAPI spec.
    #[clap(short = 'o', long, env = "CDD_OUTPUT", default_value = "spec.json")]
    pub output: PathBuf,
}

pub fn execute(args: &ToOpenApiArgs, target: &TargetMode) -> AppResult<()> {
    println!("Extracting OpenAPI specification from {:?}", args.file);

    if !args.file.exists() {
        return Err(AppError::General(format!(
            "Path not found: {:?}",
            args.file
        )));
    }

    let mut parsed_models = Vec::new();
    let mut parsed_routes = Vec::new();

    // Walk directory and parse models and routes
    let walker = WalkDir::new(&args.file).into_iter();
    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "rs") {
            if let Ok(content) = fs::read_to_string(path) {
                // Parse models
                if let Ok(struct_names) = extract_struct_names(&content) {
                    for name in struct_names {
                        if let Ok(model) = extract_model(&content, &name) {
                            parsed_models.push(model);
                        }
                    }
                }

                // Parse routes
                match target {
                    TargetMode::Server => {
                        if let Ok(routes) = parse_actix_routes(&content) {
                            parsed_routes.extend(routes);
                        }
                    }
                    TargetMode::Client | TargetMode::Cli => {
                        if let Ok(routes) = parse_reqwest_routes(&content) {
                            parsed_routes.extend(routes);
                        }
                    }
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

    let is_json = args.output.extension().is_some_and(|ext| ext == "json");
    let output_str = if is_json {
        serde_json::to_string_pretty(&doc)
            .map_err(|e| AppError::General(format!("Serialization failed: {}", e)))?
    } else {
        serde_yaml::to_string(&doc)
            .map_err(|e| AppError::General(format!("Serialization failed: {}", e)))?
    };

    fs::write(&args.output, output_str).map_err(|e| {
        AppError::General(format!("Failed to write to file {:?}: {}", args.output, e))
    })?;

    println!("OpenAPI spec successfully written to {:?}", args.output);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_to_openapi_execute() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();

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
            .unwrap()
            .write_all(rs_code.as_bytes())
            .unwrap();

        let args = ToOpenApiArgs {
            file: src_dir,
            output: dir.path().join("spec.json"),
        };

        let result = execute(&args, &TargetMode::Server);
        assert!(result.is_ok());
    }

    #[test]
    fn test_to_openapi_file_not_found() {
        let args = ToOpenApiArgs {
            file: PathBuf::from("does_not_exist_dir"),
            output: PathBuf::from("spec.json"),
        };
        let result = execute(&args, &TargetMode::Server);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_openapi_client_and_cli_targets() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        let rs_path = src_dir.join("clients.rs");
        let rs_code = r#"
        /// @OAS_METHOD: GET
        /// @OAS_PATH: /users
        pub async fn get_users(client: &reqwest::Client, base_url: &str) -> Result<reqwest::Response, reqwest::Error> {
            todo!()
        }
        "#;
        File::create(&rs_path)
            .unwrap()
            .write_all(rs_code.as_bytes())
            .unwrap();

        let args = ToOpenApiArgs {
            file: src_dir.clone(),
            output: dir.path().join("spec.json"),
        };

        let result_client = execute(&args, &TargetMode::Client);
        if let Err(e) = &result_client {
            println!("Error: {}", e);
        }
        assert!(result_client.is_ok());

        let result_cli = execute(&args, &TargetMode::Cli);
        assert!(result_cli.is_ok());
    }
}
