#![deny(missing_docs)]

//! # Schema Generation Command
//!
//! Generates a JSON Schema document from a Rust struct or enum.
//! This implements the "Reflect" workflow (Rust -> Schema).

use cdd_core::error::{AppError, AppResult};
use cdd_core::parser::extract_model;
use cdd_core::schema_generator::generate_json_schema;
use std::fs;
use std::path::PathBuf;

/// Arguments for the schema-gen command.
#[derive(clap::Args, Debug, Clone)]
pub struct SchemaGenArgs {
    /// Path to the Rust source file containing the model.
    #[clap(long)]
    pub source_path: PathBuf,

    /// Name of the Struct or Enum to generate schema for.
    #[clap(long)]
    pub name: String,

    /// Output path for the schema file.
    /// Supports .json and .yaml/.yml extensions.
    /// If not provided, prints JSON to stdout.
    #[clap(long)]
    pub output: Option<PathBuf>,

    /// Optional JSON Schema Dialect URI to include in $schema.
    /// Example: `"https://json-schema.org/draft/2020-12/schema"`
    #[clap(long)]
    pub dialect: Option<String>,
}

/// Executes the schema generation.
///
/// # Arguments
///
/// * `args` - Command arguments.
pub fn execute(args: &SchemaGenArgs) -> AppResult<()> {
    if !args.source_path.exists() {
        return Err(AppError::General(format!(
            "Source file not found: {:?}",
            args.source_path
        )));
    }

    let content = fs::read_to_string(&args.source_path)
        .map_err(|e| AppError::General(format!("Failed to read source file: {}", e)))?;

    // 1. Parse Rust Model using Core Parser
    // This utilizes ra_ap_syntax to find the specific struct/enum definition
    let model = extract_model(&content, &args.name)?;

    // 2. Generate JSON Schema using Core Generator
    // This maps Rust types (i32, Option<T>, Vec<T>) to Schema types
    let schema_json = generate_json_schema(&model, args.dialect.as_deref())?;

    // 3. Output formatting
    let output_str = if let Some(out_path) = &args.output {
        let ext = out_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("json");
        match ext {
            "yaml" | "yml" => serde_yaml::to_string(&schema_json)
                .map_err(|e| AppError::General(format!("YAML serialization failed: {}", e)))?,
            _ => serde_json::to_string_pretty(&schema_json)
                .map_err(|e| AppError::General(format!("JSON serialization failed: {}", e)))?,
        }
    } else {
        // Stdout defaults to JSON
        serde_json::to_string_pretty(&schema_json)
            .map_err(|e| AppError::General(format!("JSON serialization failed: {}", e)))?
    };

    // 4. Write result
    if let Some(out_path) = &args.output {
        if let Some(parent) = out_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    AppError::General(format!("Failed to create output directory: {}", e))
                })?;
            }
        }
        fs::write(out_path, output_str)
            .map_err(|e| AppError::General(format!("Failed to write output file: {}", e)))?;
        println!("Schema generated at {:?}", out_path);
    } else {
        println!("{}", output_str);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_schema_gen_struct_to_json() {
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("model.rs");
        let out_path = dir.path().join("schema.json");

        let rust_code = r#"
            /// A User struct
            struct User {
                id: i32,
                name: String,
                is_active: bool
            }
        "#;

        fs::File::create(&src_path)
            .unwrap()
            .write_all(rust_code.as_bytes())
            .unwrap();

        let args = SchemaGenArgs {
            source_path: src_path,
            name: "User".to_string(),
            output: Some(out_path.clone()),
            dialect: None,
        };

        execute(&args).unwrap();

        let json_content = fs::read_to_string(&out_path).unwrap();
        assert!(json_content.contains("\"title\": \"User\""));
        assert!(json_content.contains("\"description\": \"A User struct\""));
        assert!(json_content.contains("\"type\": \"integer\"")); // id
        assert!(json_content.contains("\"type\": \"boolean\"")); // is_active
    }

    #[test]
    fn test_schema_gen_enum_to_yaml() {
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("enums.rs");
        let out_path = dir.path().join("schema.yaml");

        let rust_code = r#"
            enum Status {
                Active,
                Inactive
            }
        "#;

        fs::File::create(&src_path)
            .unwrap()
            .write_all(rust_code.as_bytes())
            .unwrap();

        let args = SchemaGenArgs {
            source_path: src_path,
            name: "Status".to_string(),
            output: Some(out_path.clone()),
            dialect: None,
        };

        execute(&args).unwrap();

        let yaml_content = fs::read_to_string(&out_path).unwrap();
        // YAML specific checking
        assert!(yaml_content.contains("title: Status"));
        assert!(yaml_content.contains("oneOf:"));
    }

    #[test]
    fn test_schema_gen_not_found() {
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("empty.rs");
        fs::File::create(&src_path).unwrap().write_all(b"").unwrap();

        let args = SchemaGenArgs {
            source_path: src_path,
            name: "NonExistent".to_string(),
            output: None,
            dialect: None,
        };

        let result = execute(&args);
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::General(msg) => assert!(msg.contains("Model 'NonExistent' not found")),
            _ => panic!("Wrong error type"),
        }
    }
}
