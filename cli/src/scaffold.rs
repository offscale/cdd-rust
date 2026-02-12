#![deny(missing_docs)]

//! # Scaffold Command
//!
//! Generates Rust handler scaffolding and route registration from OpenAPI specifications.
//!
//! This command:
//! 1. Parses the OpenAPI file (OAS 3.x or Swagger 2.0).
//! 2. Groups routes by their first Tag (or "default" if missing).
//! 3. Generates or Updates module files (e.g., `handlers/users.rs`) with `async fn` signatures.
//! 4. (Optional) Injects route registrations into a shared config function (e.g., in `lib.rs`).

use cdd_core::handler_generator::update_handler_module;
use cdd_core::oas::{parse_openapi_routes, ParsedRoute};
use cdd_core::route_generator::register_routes;
use cdd_core::strategies::BackendStrategy;
use cdd_core::{AppError, AppResult};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Arguments for the scaffold command.
#[derive(clap::Args, Debug, Clone)]
pub struct ScaffoldArgs {
    /// Path to the OpenAPI spec.
    #[clap(long, default_value = "docs/openapi.yaml")]
    pub openapi_path: PathBuf,

    /// Output directory for handler modules (e.g., `web/src/http/handlers`).
    #[clap(long, default_value = "web/src/http/handlers")]
    pub output_dir: PathBuf,

    /// Path to the file containing the route configuration function.
    /// If provided, routes will be injected into `pub fn config(cfg: &mut web::ServiceConfig)`.
    /// Example: `web/src/lib.rs` or `web/src/http/routes.rs`.
    #[clap(long)]
    pub route_config_path: Option<PathBuf>,

    /// Whether to force overwrite existing files (by default, it patches them).
    /// Note: The patcher is non-destructive by default.
    #[clap(long)]
    pub force: bool,
}

/// Executes the scaffolding process.
///
/// # Arguments
///
/// * `args` - Command arguments including paths.
/// * `strategy` - The backend strategy (e.g. Actix) used for code generation.
pub fn execute(args: &ScaffoldArgs, strategy: &impl BackendStrategy) -> AppResult<()> {
    println!("Scaffolding handlers from {:?}...", args.openapi_path);

    if !args.openapi_path.exists() {
        return Err(AppError::General(format!(
            "OpenAPI file not found: {:?}",
            args.openapi_path
        )));
    }

    // 1. Parse Routes
    let yaml_content = fs::read_to_string(&args.openapi_path)
        .map_err(|e| AppError::General(format!("Failed to read OpenAPI: {}", e)))?;

    let routes = parse_openapi_routes(&yaml_content)?;
    if routes.is_empty() {
        println!("No routes found in OpenAPI spec.");
        return Ok(());
    }

    // 2. Group Routes by Tag
    let grouped = group_routes_by_tag(&routes);

    // 3. Ensure Output Directory
    if !args.output_dir.exists() {
        fs::create_dir_all(&args.output_dir)
            .map_err(|e| AppError::General(format!("Failed to create output directory: {}", e)))?;
    }

    // 4. Generate/Update Handler Moduels
    for (tag, module_routes) in &grouped {
        let filename = format!("{}.rs", to_snake_case(tag));
        let file_path = args.output_dir.join(&filename);

        println!(
            "  -> Processing module: {} ({} routes)",
            filename,
            module_routes.len()
        );

        let existing_content = if file_path.exists() {
            fs::read_to_string(&file_path)
                .map_err(|e| AppError::General(format!("Failed to read existing file: {}", e)))?
        } else {
            String::new()
        };

        let new_content = update_handler_module(&existing_content, module_routes, strategy)?;

        if existing_content != new_content {
            fs::write(&file_path, new_content).map_err(|e| {
                AppError::General(format!("Failed to write file {:?}: {}", file_path, e))
            })?;
        }
    }

    // 5. Inject Routes into Config (Optional)
    if let Some(config_path) = &args.route_config_path {
        println!("  -> Injecting route registrations into {:?}", config_path);

        // Ensure parent dir exists if creating new config file
        if let Some(parent) = config_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    AppError::General(format!("Failed to create config directory: {}", e))
                })?;
            }
        }

        let mut config_source = if config_path.exists() {
            fs::read_to_string(config_path)
                .map_err(|e| AppError::General(format!("Failed to read config file: {}", e)))?
        } else {
            String::new()
        };

        // We iterate tags to register modules in the config.
        // Sort keys to ensure deterministic output order for generated code.
        let mut sorted_tags: Vec<_> = grouped.keys().collect();
        sorted_tags.sort();

        for tag in sorted_tags {
            let module_routes = &grouped[tag];
            let module_name = to_snake_case(tag);

            // Calls core::route_generator::register_routes
            // This function parses the AST of `config_path` and injects `cfg.service(...)`
            // avoiding duplicates.
            config_source = register_routes(&config_source, &module_name, module_routes, strategy)?;
        }

        fs::write(config_path, config_source)
            .map_err(|e| AppError::General(format!("Failed to update config file: {}", e)))?;
    }

    Ok(())
}

/// Helper to group routes by their primary tag.
/// Routes without tags default to "default".
fn group_routes_by_tag(routes: &[ParsedRoute]) -> HashMap<String, Vec<ParsedRoute>> {
    let mut map: HashMap<String, Vec<ParsedRoute>> = HashMap::new();

    for route in routes {
        // Use the first tag as the grouping key, or "default"
        let tag = route
            .tags
            .first()
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        map.entry(tag).or_default().push(route.clone());
    }

    map
}

/// Wrapper for core snake_case utility specific to module naming conventions.
fn to_snake_case(s: &str) -> String {
    cdd_core::oas::routes::naming::to_snake_case(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cdd_core::strategies::ActixStrategy;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_scaffold_handlers_and_route_injection() {
        let dir = tempdir().unwrap();
        let openapi_path = dir.path().join("openapi.yaml");
        let output_dir = dir.path().join("handlers");
        let route_config_path = dir.path().join("routes.rs");

        // 1. Create a dummy OpenAPI spec (Swagger 2.0 compatible structure logic-wise via Tags)
        let yaml = r#"
openapi: 3.0.0
info: {title: Test, version: 1.0}
paths:
  /users/{id}:
    parameters:
      - name: id
        in: path
        required: true
        schema:
          type: string
    get:
      tags: [Users]
      operationId: getUser
      responses:
        '200': { description: OK }
  /posts:
    post:
      tags: [Posts]
      operationId: createPost
      responses:
        '200': { description: OK }
"#;
        let mut f = fs::File::create(&openapi_path).unwrap();
        f.write_all(yaml.as_bytes()).unwrap();

        let args = ScaffoldArgs {
            openapi_path,
            output_dir: output_dir.clone(),
            route_config_path: Some(route_config_path.clone()),
            force: false,
        };
        let strategy = ActixStrategy;

        // 2. Execute
        execute(&args, &strategy).unwrap();

        // 3. Verify Handlers Generated
        let users_file = output_dir.join("users.rs");
        let posts_file = output_dir.join("posts.rs");

        assert!(users_file.exists());
        assert!(posts_file.exists());

        let users_code = fs::read_to_string(&users_file).unwrap();
        assert!(users_code.contains("pub async fn get_user("));

        let posts_code = fs::read_to_string(&posts_file).unwrap();
        assert!(posts_code.contains("pub async fn create_post("));

        // 4. Verify Route Injection
        assert!(route_config_path.exists());
        let config_code = fs::read_to_string(&route_config_path).unwrap();

        // Check required imports for Actix
        assert!(config_code.contains("pub fn config(cfg: &mut web::ServiceConfig)"));
        assert!(config_code.contains("use crate::http::handlers;"));

        // Check registration statements
        // Note: The strategy generates `handlers::{module}::{func}`
        assert!(config_code.contains("handlers::users::get_user"));
        assert!(config_code.contains("handlers::posts::create_post"));

        // 5. Idempotency Check
        // Running it again should not duplicate lines (core logic test, but verified via CLI flow)
        execute(&args, &strategy).unwrap();
        let config_code_2 = fs::read_to_string(&route_config_path).unwrap();

        // Count occurrences of registration
        let count = config_code_2.matches("handlers::users::get_user").count();
        assert_eq!(count, 1, "Route registration should be idempotent");
    }

    #[test]
    fn test_missing_tags_defaults_to_default_module() {
        let dir = tempdir().unwrap();
        let openapi_path = dir.path().join("openapi.yaml");
        let output_dir = dir.path().join("handlers");

        let yaml = r#"
openapi: 3.0.0
info: {title: No Tag, version: 1}
paths:
  /ping:
    get:
      operationId: ping
      responses:
        '200': { description: OK }
"#;
        let mut f = fs::File::create(&openapi_path).unwrap();
        f.write_all(yaml.as_bytes()).unwrap();

        let args = ScaffoldArgs {
            openapi_path,
            output_dir: output_dir.clone(),
            route_config_path: None, // Optional: skip injection
            force: false,
        };
        execute(&args, &ActixStrategy).unwrap();

        let default_file = output_dir.join("default.rs");
        assert!(default_file.exists());

        let code = fs::read_to_string(default_file).unwrap();
        assert!(code.contains("pub async fn ping()"));
    }
}
