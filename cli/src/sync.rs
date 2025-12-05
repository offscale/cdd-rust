#![deny(missing_docs)]

//! # Sync Command
//!
//! Implements the pipeline: DB -> Diesel -> Model -> Schema -> OpenAPI.
//!
//! 1. **DB -> Diesel -> Model**: Uses `dsync` to generate Rust structs from `schema.rs`.
//! 2. **Model -> Schema**: Processing generated models to inject `#[derive(ToSchema)]` and other attributes.
//! 3. **Schema -> OpenAPI**: The resulting code is valid for `utoipa` OpenAPI generation at build/runtime.

use cdd_core::patcher::add_derive;
use cdd_core::{AppError, AppResult};
use dsync::{GenerationConfig, GenerationConfigOpts, TableOptions};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Arguments for the sync command.
#[derive(clap::Args, Debug, Clone)]
pub struct SyncArgs {
    /// Path to the Diesel schema file (e.g. web/src/schema.rs).
    #[clap(long, default_value = "web/src/schema.rs")]
    pub schema_path: PathBuf,

    /// Output directory for generated models (e.g. web/src/models).
    #[clap(long, default_value = "web/src/models")]
    pub model_dir: PathBuf,

    /// Skip the dsync generation step (only process existing files).
    #[clap(long)]
    pub no_gen: bool,
}

/// Executes the sync pipeline.
pub fn execute(args: &SyncArgs) -> AppResult<()> {
    println!("Starting Sync Pipeline...");

    // 1. DB -> Models (using dsync)
    if !args.no_gen {
        generate_models(&args.schema_path, &args.model_dir)?;
    } else {
        println!("Skipping model generation (--no-gen).");
    }

    // 2. Models -> Schema (Injecting attributes)
    process_models_for_openapi(&args.model_dir)?;

    println!("Sync Pipeline Completed successfully.");
    Ok(())
}

/// Generates Rust structs from the Diesel schema using `dsync`.
///
/// Ref replicates the configuration from Item 1.3:
/// - Disables CRUD function generation (data structs only).
/// - Adds `diesel::prelude::*`.
fn generate_models(schema_path: &Path, output_dir: &Path) -> AppResult<()> {
    if !schema_path.exists() {
        return Err(AppError::General(format!(
            "Schema file not found at: {:?}",
            schema_path
        )));
    }

    println!(
        "Generating models from {:?} into {:?}...",
        schema_path, output_dir
    );

    // GenerationConfig does not implement Default, so we must construct it fully.
    // connection_type is required, defaulting to PgConnection for typical use cases.
    let config = GenerationConfig {
        connection_type: String::from("diesel::pg::PgConnection"),
        options: GenerationConfigOpts {
            default_table_options: TableOptions::default().disable_fns(),
            ..Default::default()
        },
    };

    dsync::generate_files(schema_path, output_dir, config)
        .map_err(|e| AppError::General(format!("dsync generation failed: {}", e)))?;

    Ok(())
}

/// iterators over generated model files and injects OpenAPI attributes.
///
/// - Adds `#[derive(ToSchema)]`.
/// - Adds `#[derive(Serialize, Deserialize)]`.
/// - Prepends `#![allow(missing_docs)]` if missing (since dsync output isn't documented).
fn process_models_for_openapi(model_dir: &Path) -> AppResult<()> {
    println!(
        "Processing models for OpenAPI compliance in {:?}...",
        model_dir
    );

    if !model_dir.exists() {
        return Err(AppError::General(format!(
            "Model directory not found: {:?}",
            model_dir
        )));
    }

    let walker = WalkDir::new(model_dir).into_iter();

    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();

        // Only process Rust files, skip mod.rs if we want (orphans usually valid, but typically we target struct files)
        // dsync generates mod.rs and table files.
        if path.extension().map_or(false, |ext| ext == "rs")
            && path.file_name().unwrap() != "mod.rs"
        {
            process_file(path)?;
        }
    }

    Ok(())
}

fn process_file(path: &Path) -> AppResult<()> {
    let content = fs::read_to_string(path)
        .map_err(|e| AppError::General(format!("Failed to read file {:?}: {}", path, e)))?;

    let mut new_content = content.clone();

    // 1. Add Derives
    // Extract struct name (naive assumption: dsync generates one struct per file with file name or check content)
    // Actually, dsync generates struct named after table.
    // We use cdd_core::extract_struct_names to be robust.
    let struct_names = cdd_core::extract_struct_names(&new_content)?;

    for name in struct_names {
        new_content = add_derive(&new_content, &name, "ToSchema")?;
        new_content = add_derive(&new_content, &name, "Serialize")?;
        new_content = add_derive(&new_content, &name, "Deserialize")?;
    }

    // 2. Add File HeaderAttributes (allow missing docs)
    // dsync files are auto-generated.
    let lint_allow = "#![allow(missing_docs)]\n";
    if !new_content.contains("#![allow(missing_docs)]") {
        new_content = format!("{}{}", lint_allow, new_content);
    }

    // 3. Add Imports if needed
    // ToSchema needs utoipa. Serialize/Deserialize need serde.
    // If not present, inject them at the top (after lint allow).
    if !new_content.contains("use utoipa::ToSchema;") {
        // Insert after first line (lint) or at start
        // Find position after first newline
        if let Some(idx) = new_content.find('\n') {
            new_content.insert_str(idx + 1, "use utoipa::ToSchema;\n");
        } else {
            new_content.push_str("use utoipa::ToSchema;\n");
        }
    }
    if !new_content.contains("use serde::{Deserialize, Serialize};") {
        if let Some(idx) = new_content.find('\n') {
            new_content.insert_str(idx + 1, "use serde::{Deserialize, Serialize};\n");
        }
    }

    if new_content != content {
        fs::write(path, new_content)
            .map_err(|e| AppError::General(format!("Failed to write file {:?}: {}", path, e)))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_process_file_injects_derives_and_imports() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("users.rs");

        // Simulating a fresh dsync output
        let initial_code = r#"
use crate::schema::users;

#[derive(Debug, Clone, Queryable, Insertable)]
#[diesel(table_name = users)]
pub struct User {
    pub id: i32,
    pub name: String,
}
"#;
        File::create(&file_path)
            .unwrap()
            .write_all(initial_code.as_bytes())
            .unwrap();

        process_file(&file_path).unwrap();

        let new_code = fs::read_to_string(&file_path).unwrap();

        assert!(new_code.contains("#![allow(missing_docs)]"));
        assert!(new_code.contains("use utoipa::ToSchema;"));
        assert!(new_code.contains("use serde::{Deserialize, Serialize};"));
        assert!(new_code.contains(
            "#[derive(Debug, Clone, Queryable, Insertable, ToSchema, Serialize, Deserialize)]"
        ));

        // Idempotency check
        process_file(&file_path).unwrap();
        let code_2 = fs::read_to_string(&file_path).unwrap();
        assert_eq!(new_code, code_2);
    }

    #[test]
    fn test_sync_no_gen() {
        // Just verify it doesn't crash if dir empty and flag set
        let args = SyncArgs {
            schema_path: PathBuf::from("fake"),
            model_dir: PathBuf::from("fake_dir"),
            no_gen: true,
        };
        // Expect error because model dir doesn't exist for processing
        let res = execute(&args);
        assert!(res.is_err());
    }
}
