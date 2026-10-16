#![deny(missing_docs)]

//! # Generator
//!
//! Definition of the ModelMapper trait and implementations for DB-to-Struct generation.

use cdd_core::error::{AppError, AppResult};
use dsync::{GenerationConfig, GenerationConfigOpts, TableOptions};
use std::path::Path;

/// Trait for generating backend models from a database schema source.
pub trait ModelMapper {
    /// Generates model files.
    ///
    /// # Arguments
    ///
    /// * `schema_path` - Path to the schema file (e.g. schema.rs).
    /// * `output_dir` - Directory to output generated code.
    fn generate(&self, schema_path: &Path, output_dir: &Path) -> AppResult<()>;
}

/// Mapper implementation using Diesel and dsync.
pub struct DieselMapper;

impl ModelMapper for DieselMapper {
    fn generate(&self, schema_path: &Path, output_dir: &Path) -> AppResult<()> {
        if !schema_path.exists() {
            return Err(AppError::General(format!(
                "Schema file not found at: {:?}",
                schema_path
            )));
        }

        // Configuration logic moved from sync.rs
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_diesel_mapper_generate_success() {
        // Setup a fake schema.rs that dsync can parse
        let dir = tempdir().expect("Failed to create temporary directory");
        let schema_path = dir.path().join("schema.rs");
        let output_dir = dir.path().join("models");
        std::fs::create_dir(&output_dir).expect("Failed to create");

        // Minimal valid diesel schema
        let schema_content = r#"
            // @generated automatically by Diesel CLI.
            diesel::table! {
                posts (id) {
                    id -> Integer,
                }
            }
        "#;
        File::create(&schema_path)
            .expect("Failed to create")
            .write_all(schema_content.as_bytes())
            .expect("Failed to write to file");

        let mapper = DieselMapper;
        let res = mapper.generate(&schema_path, &output_dir);
        assert!(res.is_ok());

        // Passing an existing regular file as output_dir causes dsync::generate_files to return Err
        let bad_res = mapper.generate(&schema_path, &schema_path);
        assert!(bad_res.is_err());
    }

    #[test]
    fn test_diesel_mapper_file_not_found() {
        let mapper = DieselMapper;
        let p = Path::new("non_existent_schema.rs");
        let out = Path::new("out");
        let res = mapper.generate(p, out);
        assert!(res
            .expect_err("expected error")
            .to_string()
            .contains("Schema file not found"));
    }
}
