#![deny(missing_docs)]

//! # Schema Generation Command
//!
//! Generates a JSON Schema document from a Rust struct or enum.
//! This implements the "Reflect" workflow (Rust -> Schema).

use cdd_core::error::{AppError, AppResult};
use cdd_core::parser::extract_model;
use cdd_core::schema_generator::{
    generate_json_schema, generate_openapi_document, OpenApiContact, OpenApiInfo, OpenApiLicense,
};
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

    /// Emit a minimal OpenAPI 3.2 document instead of a standalone JSON Schema.
    #[clap(long, default_value_t = false)]
    pub openapi: bool,

    /// Optional OpenAPI info.title override (defaults to the model name).
    #[clap(long)]
    pub info_title: Option<String>,

    /// Optional OpenAPI info.version override (defaults to "1.0.0").
    #[clap(long)]
    pub info_version: Option<String>,

    /// Optional OpenAPI info.description override.
    #[clap(long)]
    pub info_description: Option<String>,

    /// Optional OpenAPI info.summary override.
    #[clap(long)]
    pub info_summary: Option<String>,

    /// Optional OpenAPI info.termsOfService override.
    #[clap(long)]
    pub info_terms_of_service: Option<String>,

    /// Optional OpenAPI info.contact.name override.
    #[clap(long)]
    pub info_contact_name: Option<String>,

    /// Optional OpenAPI info.contact.url override.
    #[clap(long)]
    pub info_contact_url: Option<String>,

    /// Optional OpenAPI info.contact.email override.
    #[clap(long)]
    pub info_contact_email: Option<String>,

    /// Optional OpenAPI info.license.name override.
    #[clap(long)]
    pub info_license_name: Option<String>,

    /// Optional OpenAPI info.license.url override.
    #[clap(long)]
    pub info_license_url: Option<String>,

    /// Optional OpenAPI info.license.identifier override.
    #[clap(long)]
    pub info_license_identifier: Option<String>,

    /// Optional OpenAPI `$self` URI for the document base.
    #[clap(long)]
    pub self_uri: Option<String>,
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

    let output_json = if args.openapi {
        let title = args
            .info_title
            .clone()
            .unwrap_or_else(|| model.name().to_string());
        let version = args
            .info_version
            .clone()
            .unwrap_or_else(|| "1.0.0".to_string());
        let mut info = OpenApiInfo::new(title, version);
        if let Some(summary) = &args.info_summary {
            info = info.with_summary(summary.clone());
        }
        if let Some(desc) = &args.info_description {
            info = info.with_description(desc.clone());
        }
        if let Some(terms) = &args.info_terms_of_service {
            info = info.with_terms_of_service(terms.clone());
        }

        if args.info_contact_name.is_some()
            || args.info_contact_url.is_some()
            || args.info_contact_email.is_some()
        {
            let mut contact = OpenApiContact::new();
            if let Some(name) = &args.info_contact_name {
                contact = contact.with_name(name.clone());
            }
            if let Some(url) = &args.info_contact_url {
                contact = contact.with_url(url.clone());
            }
            if let Some(email) = &args.info_contact_email {
                contact = contact.with_email(email.clone());
            }
            info = info.with_contact(contact);
        }

        let has_license = args.info_license_name.is_some()
            || args.info_license_url.is_some()
            || args.info_license_identifier.is_some();
        if has_license && args.info_license_name.is_none() {
            return Err(AppError::General(
                "info_license_name is required when specifying license details".into(),
            ));
        }
        if let Some(name) = &args.info_license_name {
            let mut license = OpenApiLicense::new(name.clone());
            if let Some(identifier) = &args.info_license_identifier {
                license = license.with_identifier(identifier.clone());
            }
            if let Some(url) = &args.info_license_url {
                license = license.with_url(url.clone());
            }
            info = info.with_license(license);
        }
        if let Some(self_uri) = &args.self_uri {
            info = info.with_self_uri(self_uri.clone());
        }
        generate_openapi_document(&model, args.dialect.as_deref(), &info)?
    } else {
        // 2. Generate JSON Schema using Core Generator
        // This maps Rust types (i32, Option<T>, Vec<T>) to Schema types
        generate_json_schema(&model, args.dialect.as_deref())?
    };

    // 3. Output formatting
    let output_str = if let Some(out_path) = &args.output {
        let ext = out_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("json");
        match ext {
            "yaml" | "yml" => serde_yaml::to_string(&output_json)
                .map_err(|e| AppError::General(format!("YAML serialization failed: {}", e)))?,
            _ => serde_json::to_string_pretty(&output_json)
                .map_err(|e| AppError::General(format!("JSON serialization failed: {}", e)))?,
        }
    } else {
        // Stdout defaults to JSON
        serde_json::to_string_pretty(&output_json)
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
            openapi: false,
            info_title: None,
            info_version: None,
            info_description: None,
            info_summary: None,
            info_terms_of_service: None,
            info_contact_name: None,
            info_contact_url: None,
            info_contact_email: None,
            info_license_name: None,
            info_license_url: None,
            info_license_identifier: None,
            self_uri: None,
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
            openapi: false,
            info_title: None,
            info_version: None,
            info_description: None,
            info_summary: None,
            info_terms_of_service: None,
            info_contact_name: None,
            info_contact_url: None,
            info_contact_email: None,
            info_license_name: None,
            info_license_url: None,
            info_license_identifier: None,
            self_uri: None,
        };

        execute(&args).unwrap();

        let yaml_content = fs::read_to_string(&out_path).unwrap();
        // YAML specific checking
        assert!(yaml_content.contains("title: Status"));
        assert!(yaml_content.contains("oneOf:"));
    }

    #[test]
    fn test_schema_gen_openapi_wrap() {
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("model.rs");
        let out_path = dir.path().join("openapi.json");

        let rust_code = r#"
            struct Widget {
                id: i32,
            }
        "#;

        fs::File::create(&src_path)
            .unwrap()
            .write_all(rust_code.as_bytes())
            .unwrap();

        let args = SchemaGenArgs {
            source_path: src_path,
            name: "Widget".to_string(),
            output: Some(out_path.clone()),
            dialect: None,
            openapi: true,
            info_title: Some("Widget API".to_string()),
            info_version: Some("9.9.9".to_string()),
            info_description: Some("Docs".to_string()),
            info_summary: None,
            info_terms_of_service: None,
            info_contact_name: None,
            info_contact_url: None,
            info_contact_email: None,
            info_license_name: None,
            info_license_url: None,
            info_license_identifier: None,
            self_uri: None,
        };

        execute(&args).unwrap();

        let json_content = fs::read_to_string(&out_path).unwrap();
        assert!(json_content.contains("\"openapi\": \"3.2.0\""));
        assert!(json_content.contains("\"title\": \"Widget API\""));
        assert!(json_content.contains("\"version\": \"9.9.9\""));
        assert!(json_content.contains("\"Widget\""));
        assert!(json_content.contains("\"components\""));
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
            openapi: false,
            info_title: None,
            info_version: None,
            info_description: None,
            info_summary: None,
            info_terms_of_service: None,
            info_contact_name: None,
            info_contact_url: None,
            info_contact_email: None,
            info_license_name: None,
            info_license_url: None,
            info_license_identifier: None,
            self_uri: None,
        };

        let result = execute(&args);
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::General(msg) => assert!(msg.contains("Model 'NonExistent' not found")),
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_schema_gen_license_requires_name() {
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("model.rs");
        fs::File::create(&src_path)
            .unwrap()
            .write_all(b"struct User { id: i32 }")
            .unwrap();

        let args = SchemaGenArgs {
            source_path: src_path,
            name: "User".to_string(),
            output: None,
            dialect: None,
            openapi: true,
            info_title: Some("User API".to_string()),
            info_version: Some("1.0.0".to_string()),
            info_description: None,
            info_summary: None,
            info_terms_of_service: None,
            info_contact_name: None,
            info_contact_url: None,
            info_contact_email: None,
            info_license_name: None,
            info_license_url: Some("https://example.com/license".to_string()),
            info_license_identifier: None,
            self_uri: None,
        };

        let result = execute(&args);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("info_license_name"));
    }
}
