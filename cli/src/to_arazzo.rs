#![deny(missing_docs)]

//! Module for extracting Arazzo 1.1.0 specifications from Rust source code or crate directories.

use cdd_core::arazzo::emit::{serialize_arazzo_document, ArazzoFormat};
use cdd_core::arazzo::models::{
    ArazzoDocument, ArazzoInfo, ArazzoVersion, SourceDescription, SourceDescriptionType,
};
use cdd_core::arazzo::parse::{extract_arazzo_document_from_rust, extract_workflows_from_rust};
use cdd_core::error::{AppError, AppResult};
use clap::Args;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

/// Arguments for the `to_arazzo` CLI command.
#[derive(Args, Debug, Clone)]
pub struct ToArazzoArgs {
    /// Input Rust source file or crate directory.
    #[clap(short = 'i', long, env = "CDD_INPUT")]
    pub input: PathBuf,

    /// Output file path for the generated Arazzo 1.1.0 specification (`.yaml`, `.yml`, or `.json`).
    #[clap(short = 'o', long, default_value = "arazzo.yaml", env = "CDD_OUTPUT")]
    pub output: PathBuf,

    /// Optional title override for `info.title`.
    #[clap(long, env = "CDD_ARAZZO_TITLE")]
    pub title: Option<String>,

    /// Optional version override for `info.version`.
    #[clap(long, env = "CDD_ARAZZO_VERSION")]
    pub version: Option<String>,

    /// Optional fallback OpenAPI `sourceDescriptions` URL if none is annotated in source.
    #[clap(long, env = "CDD_ARAZZO_SOURCE_URL")]
    pub source_url: Option<String>,
}

/// Executes the `to_arazzo` CLI command, statically extracting workflows from Rust AST
/// and emitting a canonical Arazzo 1.1.0 YAML or JSON specification.
///
/// # Arguments
/// * `args` - Parsed [`ToArazzoArgs`].
///
/// # Returns
/// `Ok(())` when the Arazzo 1.1.0 specification has been written to `args.output`.
///
/// # Errors
/// Returns [`AppError`] if the input path does not exist, contains no workflows, or fails serialization.
pub fn run_to_arazzo(args: &ToArazzoArgs) -> AppResult<()> {
    if !args.input.exists() {
        return Err(AppError::General(format!(
            "Input path {:?} does not exist.",
            args.input
        )));
    }

    let doc = if args.input.is_file() {
        let code = fs::read_to_string(&args.input)?;
        extract_arazzo_document_from_rust(
            &code,
            args.title.as_deref(),
            args.version.as_deref(),
            args.source_url.as_deref(),
        )?
    } else {
        extract_arazzo_from_directory(args)?
    };

    let is_json = args
        .output
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
    let format = if is_json {
        ArazzoFormat::Json
    } else {
        ArazzoFormat::Yaml
    };

    let content = serialize_arazzo_document(&doc, format)?;
    let _ = args.output.parent().map(fs::create_dir_all);
    fs::write(&args.output, content)?;
    Ok(())
}

fn extract_arazzo_from_directory(args: &ToArazzoArgs) -> AppResult<ArazzoDocument> {
    let mut rust_files = Vec::new();
    for entry in WalkDir::new(&args.input).into_iter().flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("rs") {
            rust_files.push(path.to_path_buf());
        }
    }

    let mut all_workflows = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    for file in rust_files {
        let code = fs::read_to_string(&file)?;
        if code.contains("@ARAZZO_SPEC_JSON:") {
            return extract_arazzo_document_from_rust(
                &code,
                args.title.as_deref(),
                args.version.as_deref(),
                args.source_url.as_deref(),
            );
        }
        for wf in extract_workflows_from_rust(&code)? {
            if seen_ids.insert(wf.workflow_id.clone()) {
                all_workflows.push(wf);
            }
        }
    }

    if all_workflows.is_empty() {
        return Err(AppError::ArazzoValidation {
            path: "/workflows".to_string(),
            reason: "No Arazzo workflows found in Rust directory".to_string(),
        });
    }

    Ok(ArazzoDocument {
        arazzo: ArazzoVersion::V1_1_0,
        self_uri: None,
        info: ArazzoInfo {
            title: args
                .title
                .clone()
                .unwrap_or(String::from("Extracted Rust Workflows")),
            summary: None,
            description: None,
            version: args.version.clone().unwrap_or(String::from("0.1.0")),
            extensions: BTreeMap::new(),
        },
        source_descriptions: vec![SourceDescription {
            name: "default_api".to_string(),
            url: args
                .source_url
                .clone()
                .unwrap_or(String::from("./openapi.yaml")),
            source_type: Some(SourceDescriptionType::OpenApi),
            extensions: BTreeMap::new(),
        }],
        workflows: all_workflows,
        components: None,
        extensions: BTreeMap::new(),
    })
}

/// Programmatic configuration for `to_arazzo`.
#[derive(Debug, Default, Clone)]
pub struct ToArazzoConfig {
    /// Input Rust file or directory path.
    pub input: PathBuf,
    /// Output Arazzo specification path.
    pub output: PathBuf,
    /// Optional title override.
    pub title: Option<String>,
    /// Optional version override.
    pub version: Option<String>,
    /// Optional fallback OpenAPI source URL.
    pub source_url: Option<String>,
}

/// Programmatic entrypoint for extracting an Arazzo 1.1.0 specification from Rust code.
///
/// # Arguments
/// * `config` - [`ToArazzoConfig`] parameters.
///
/// # Returns
/// `Ok(())` on success.
///
/// # Errors
/// Returns [`AppError`] if extraction or writing fails.
pub fn to_arazzo(config: &ToArazzoConfig) -> AppResult<()> {
    run_to_arazzo(&ToArazzoArgs {
        input: config.input.clone(),
        output: config.output.clone(),
        title: config.title.clone(),
        version: config.version.clone(),
        source_url: config.source_url.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::from_arazzo::{from_arazzo, FromArazzoConfig, FromArazzoSubcommand};
    use cdd_core::arazzo::parse::parse_arazzo_document;
    use tempfile::tempdir;

    #[test]
    fn test_to_arazzo_from_file_and_directory_yaml_and_json() {
        let dir = tempdir().expect("tempdir");
        let spec_in = dir.path().join("in.arazzo.yaml");
        let raw_yaml = r#"
arazzo: 1.1.0
info:
  title: Roundtrip Test
  version: 1.0.0
sourceDescriptions:
  - name: api
    url: ./openapi.yaml
    type: openapi
workflows:
  - workflowId: flowOne
    steps:
      - stepId: stepOne
        operationId: opOne
"#;
        fs::write(&spec_in, raw_yaml).expect("write in");

        let sdk_dir = dir.path().join("sdk_crate");
        from_arazzo(&FromArazzoConfig {
            subcommand: FromArazzoSubcommand::ToSdk,
            input: Some(spec_in),
            output_dir: Some(sdk_dir.clone()),
            ..Default::default()
        })
        .expect("from_arazzo");

        let out_yaml = dir.path().join("out").join("arazzo.yaml");
        to_arazzo(&ToArazzoConfig {
            input: sdk_dir.join("src/workflows.rs"),
            output: out_yaml.clone(),
            title: None,
            version: None,
            source_url: None,
        })
        .expect("to_arazzo file yaml");
        let reparsed_yaml =
            parse_arazzo_document(&fs::read_to_string(&out_yaml).expect("read")).expect("parse");
        assert_eq!(reparsed_yaml.info.title, "Roundtrip Test");

        let out_json = dir.path().join("out").join("arazzo.json");
        to_arazzo(&ToArazzoConfig {
            input: sdk_dir,
            output: out_json.clone(),
            title: Some("Custom".to_string()),
            version: Some("2.0.0".to_string()),
            source_url: None,
        })
        .expect("to_arazzo dir json");
        let reparsed_json =
            parse_arazzo_document(&fs::read_to_string(&out_json).expect("read")).expect("parse");
        assert_eq!(reparsed_json.info.title, "Custom");

        // Test handwritten Rust directory without @ARAZZO_SPEC_JSON
        let hw_dir = dir.path().join("hw_crate");
        fs::create_dir_all(&hw_dir).expect("mkdir");
        fs::write(
            hw_dir.join("wf.rs"),
            "pub async fn workflow_custom() { call().await; }",
        )
        .expect("write hw");
        fs::write(
            hw_dir.join("wf_dup.rs"),
            "pub async fn workflow_custom() { call().await; }",
        )
        .expect("write hw dup");
        fs::write(hw_dir.join("readme.md"), "not rust").expect("write md");
        let hw_out = dir.path().join("hw.yaml");
        to_arazzo(&ToArazzoConfig {
            input: hw_dir,
            output: hw_out.clone(),
            title: None,
            version: None,
            source_url: None,
        })
        .expect("to_arazzo hw dir");
        assert!(hw_out.exists());

        // Error cases: non-existent path & empty dir
        assert!(to_arazzo(&ToArazzoConfig {
            input: dir.path().join("does_not_exist"),
            output: hw_out.clone(),
            ..Default::default()
        })
        .is_err());

        let empty_dir = dir.path().join("empty_crate");
        fs::create_dir_all(&empty_dir).expect("mkdir");
        assert!(to_arazzo(&ToArazzoConfig {
            input: empty_dir,
            output: hw_out,
            ..Default::default()
        })
        .is_err());
    }
}
