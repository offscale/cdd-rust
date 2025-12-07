#![deny(missing_docs)]

//! # Sync Command
//!
//! Implements the pipeline: DB -> Diesel -> Model -> Schema -> OpenAPI.
//!
//! 1. **DB -> Diesel -> Model**: Uses `dsync` via the provided `ModelMapper`.
//! 2. **Model -> Schema**: Processing generated models to inject `#[derive(ToSchema)]` and other attributes.
//! 3. **Type Enforcement**: Optionally patches field types to strictly enforce API contract standards (e.g. `DateTime<Utc>`).
//! 3. **Schema -> OpenAPI**: The resulting code is valid for `utoipa` OpenAPI generation at build/runtime.

use crate::generator::ModelMapper;
use cdd_core::patcher::{add_derive, modify_struct_field_type};
use cdd_core::{AppError, AppResult};
use std::collections::HashMap;
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

    /// Enforce specific types for fields matching a name.
    /// Format: `"field_name=RustType"`.
    /// Example: `"--force-type created_at=chrono::DateTime<Utc>"`
    #[clap(long, value_parser = parse_key_val)]
    pub force_type: Vec<(String, String)>,
}

/// Helper to parse "key=value" arguments.
fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{}`", s))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

/// Executes the sync pipeline.
///
/// # Arguments
///
/// * `args` - Command arguments.
/// * `mapper` - The strategy/mapper used to generate models (e.g. Diesel/dsync).
pub fn execute(args: &SyncArgs, mapper: &impl ModelMapper) -> AppResult<()> {
    println!("Starting Sync Pipeline...");

    // 1. DB -> Models (using decoupled Mapper)
    if !args.no_gen {
        println!(
            "Generating models from {:?} into {:?}...",
            args.schema_path, args.model_dir
        );

        mapper
            .generate(&args.schema_path, &args.model_dir)
            .map_err(|e| AppError::General(e.to_string()))?;
    } else {
        println!("Skipping model generation (--no-gen).");
    }

    // Convert vec of tuples to HashMap for lookup
    let type_overrides: HashMap<String, String> = args.force_type.iter().cloned().collect();

    // 2. Models -> Schema (Injecting attributes & Enforcing Types)
    process_models_for_openapi(&args.model_dir, &type_overrides)?;

    println!("Sync Pipeline Completed successfully.");
    Ok(())
}

/// iterators over generated model files and injects OpenAPI attributes and patches types.
///
/// - Adds `#[derive(ToSchema)]`.
/// - Adds `#[derive(Serialize, Deserialize)]`.
/// - Patches field types if configured.
/// - Prepends `#![allow(missing_docs)]` if missing.
fn process_models_for_openapi(
    model_dir: &Path,
    type_overrides: &HashMap<String, String>,
) -> AppResult<()> {
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

        // Only process Rust files, skip mod.rs (orphans usually valid)
        if path.extension().is_some_and(|ext| ext == "rs") && path.file_name().unwrap() != "mod.rs"
        {
            process_file(path, type_overrides)?;
        }
    }

    Ok(())
}

fn process_file(path: &Path, type_overrides: &HashMap<String, String>) -> AppResult<()> {
    let content = fs::read_to_string(path)
        .map_err(|e| AppError::General(format!("Failed to read file {:?}: {}", path, e)))?;

    let mut new_content = content.clone();

    // 1. Extract struct names to act upon
    // (dsync typically creates one struct per file named after the file/table)
    let struct_names = cdd_core::extract_struct_names(&new_content)?;

    for name in &struct_names {
        // A. Add Derives
        new_content = add_derive(&new_content, name, "ToSchema")?;
        new_content = add_derive(&new_content, name, "Serialize")?;
        new_content = add_derive(&new_content, name, "Deserialize")?;

        // B. Enforce Types
        if !type_overrides.is_empty() {
            // We need to check fields of this struct.
            // Note: `modify_struct_field_type` handles `find_struct` internally,
            // but effectively we are re-parsing. Optimally we'd do it once, but patching strings
            // requires recalculating offsets if length changes, so iterative patching is robust.
            // We blindly try to patch fields if they exist in the overrides.
            for (field, new_type) in type_overrides {
                // We attempt modification. If the field doesn't exist, `modify_struct_field_type` returns Error.
                // We should check existence first or ignore specific errors.
                // For robustness in this bulk tool, checking fields first is safer.
                match cdd_core::extract_struct_fields(&new_content, name) {
                    Ok(fields) => {
                        if fields.iter().any(|f| f.name == *field) {
                            match modify_struct_field_type(&new_content, name, field, new_type) {
                                Ok(patched) => new_content = patched,
                                Err(e) => {
                                    // Log warning but don't crash pipeline if patch fails for logic reasons
                                    // (e.g. syntax issue in intermediate step), though unexpected here.
                                    eprintln!(
                                        "Warning: Failed to patch field '{}' in struct '{}': {}",
                                        field, name, e
                                    );
                                }
                            }
                        }
                    }
                    Err(_) => continue, // Struct parse failed, skip
                }
            }
        }
    }

    // 2. Add File Header Attributes (allow missing docs)
    let lint_allow = "#![allow(missing_docs)]\n";
    if !new_content.contains("#![allow(missing_docs)]") {
        new_content = format!("{}{}", lint_allow, new_content);
    }

    // 3. Add Imports if needed
    if !new_content.contains("use utoipa::ToSchema;") {
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
    use crate::generator::DieselMapper;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_process_file_injects_derives_and_imports() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("users.rs");

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

        let overrides = HashMap::new();
        process_file(&file_path, &overrides).unwrap();

        let new_code = fs::read_to_string(&file_path).unwrap();

        assert!(new_code.contains("#![allow(missing_docs)]"));
        assert!(new_code.contains("use utoipa::ToSchema;"));
        assert!(new_code.contains("use serde::{Deserialize, Serialize};"));
        assert!(new_code.contains(
            "#[derive(Debug, Clone, Queryable, Insertable, ToSchema, Serialize, Deserialize)]"
        ));
    }

    #[test]
    fn test_process_file_enforces_types() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("posts.rs");

        // dsync defaults timestamps to NaiveDateTime usually
        let initial_code = r#"
use crate::schema::posts;

#[derive(Debug, Queryable)]
pub struct Post {
    pub id: i32,
    pub created_at: chrono::NaiveDateTime,
}
"#;
        File::create(&file_path)
            .unwrap()
            .write_all(initial_code.as_bytes())
            .unwrap();

        let mut overrides = HashMap::new();
        overrides.insert(
            "created_at".to_string(),
            "chrono::DateTime<Utc>".to_string(),
        );

        process_file(&file_path, &overrides).unwrap();

        let new_code = fs::read_to_string(&file_path).unwrap();

        // Check Derives presence implies process ran
        assert!(new_code.contains("ToSchema"));
        // Check Type Replacement
        assert!(new_code.contains("pub created_at: chrono::DateTime<Utc>"));
        assert!(!new_code.contains("pub created_at: chrono::NaiveDateTime"));
    }

    #[test]
    fn test_sync_no_gen() {
        let args = SyncArgs {
            schema_path: PathBuf::from("fake"),
            model_dir: PathBuf::from("fake_dir"),
            no_gen: true,
            force_type: vec![],
        };
        // Expect error because model dir doesn't exist for processing
        let mapper = DieselMapper;
        let res = execute(&args, &mapper);
        assert!(res.is_err());
    }

    #[test]
    fn test_argument_parsing() {
        let valid = parse_key_val("id=uuid::Uuid").unwrap();
        assert_eq!(valid, ("id".to_string(), "uuid::Uuid".to_string()));

        let invalid = parse_key_val("invalid");
        assert!(invalid.is_err());
    }
}
