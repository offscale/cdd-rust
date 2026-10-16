#![deny(missing_docs)]

//! Documentation JSON generation module.
//!
//! Generates sample code snippets for OpenAPI operations by delegating directly
//! to the `cdd_core::strategies` and `cdd_core::functions::emit` implementations
//! so documentation examples stay in sync with generated code.

use crate::from_openapi::read_spec_source;
use cdd_core::error::{AppError, AppResult};
use cdd_core::functions::emit::update_handler_module;
use cdd_core::openapi::parse::document::parse_openapi_document;
use cdd_core::openapi::parse::models::{ParsedRoute, RouteKind};
use cdd_core::strategies::{ActixStrategy, AxumStrategy, BackendStrategy, ReqwestStrategy};
use clap::Args;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Arguments for the `to_docs_json` subcommand.
#[derive(Args, Debug, Clone)]
pub struct ToDocsJsonArgs {
    /// Path or URL to the OpenAPI specification.
    #[clap(short = 'i', long, env = "CDD_INPUT")]
    pub input: String,

    /// Omit import statements from generated code snippets.
    #[clap(long, default_value_t = false, env = "CDD_NO_IMPORTS")]
    pub no_imports: bool,

    /// Omit wrapping boilerplate (`fn main` / function signature) from generated code snippets.
    #[clap(long, default_value_t = false, env = "CDD_NO_WRAPPING")]
    pub no_wrapping: bool,

    /// Optional output file path (writes to stdout if omitted).
    #[clap(short = 'o', long, env = "CDD_OUTPUT")]
    pub output: Option<String>,
}

/// Output structure containing endpoints and operations for documentation generation.
#[derive(Serialize, Debug)]
pub struct DocsJsonOutput {
    /// Map of paths to HTTP methods and operation snippets.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub endpoints: BTreeMap<String, BTreeMap<String, OperationSnippets>>,
    /// Map of webhook names to HTTP methods and operation snippets.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub webhooks: BTreeMap<String, BTreeMap<String, OperationSnippets>>,
}

/// Code snippets for a single operation across supported frameworks.
#[derive(Serialize, Debug)]
pub struct OperationSnippets {
    ///Reqwest client snippet.
    pub default: CodeComponents,
    /// Actix-web server handler snippet.
    pub actix: CodeComponents,
    /// Axum server handler snippet.
    pub axum: CodeComponents,
}

/// Individual components of a code snippet.
#[derive(Serialize, Debug)]
pub struct CodeComponents {
    /// Import statements (`None` when `--no-imports` is set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imports: Option<String>,
    /// Wrapping boilerplate (`None` when `--no-wrapping` is set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapper_start: Option<String>,
    /// Core logic / invocation snippet.
    pub code: String,
    /// Closing boilerplate (`None` when `--no-wrapping` is set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapper_end: Option<String>,
}

/// Run the `to_docs_json` command and output the JSON representation.
///
/// # Arguments
/// * `args` - CLI arguments for `to_docs_json`.
///
/// # Returns
/// `Ok(())` on success.
///
/// # Errors
/// Returns [`AppError`] if reading the specification or writing the output fails.
pub fn run_to_docs_json(args: &ToDocsJsonArgs) -> AppResult<()> {
    let json_str = generate_docs_json_str(args)?;
    if let Some(ref out_path) = args.output {
        fs::write(out_path, json_str)
            .map_err(|e| AppError::General(format!("Failed to write to {}: {}", out_path, e)))?;
    } else {
        println!("{}", json_str);
    }
    Ok(())
}

/// Splits a generated handler module produced by `update_handler_module` into
/// `(imports, wrapper_start, body_code, wrapper_end)` while honoring `no_imports` and `no_wrapping`.
fn build_strategy_snippet(
    route: &ParsedRoute,
    strategy: &impl BackendStrategy,
    default_wrapper_start: &str,
    no_imports: bool,
    no_wrapping: bool,
) -> AppResult<CodeComponents> {
    let raw_imports = strategy.handler_imports().trim().to_string();
    let full_module = update_handler_module("", std::slice::from_ref(route), strategy)?;

    // Strip the test module if one was appended by `update_handler_module`
    let without_tests = full_module
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(&full_module)
        .trim();

    // Strip imports prefix
    let handler_part = without_tests
        .strip_prefix(&raw_imports)
        .unwrap_or(without_tests)
        .trim();

    // Split function signature line (`pub async fn ... {`) from inner body and closing `}`
    let open_brace = handler_part.find('{').unwrap_or(0);
    let sig_wrapper = handler_part[..=open_brace].trim().to_string();
    let after_brace = &handler_part[open_brace + 1..];
    let inner_body = after_brace
        .trim_end()
        .strip_suffix('}')
        .unwrap_or(after_brace)
        .trim()
        .to_string();
    let closing = "}".to_string();

    let wrapper_start_str = [default_wrapper_start, sig_wrapper.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(CodeComponents {
        imports: if no_imports { None } else { Some(raw_imports) },
        wrapper_start: if no_wrapping {
            None
        } else {
            Some(wrapper_start_str)
        },
        code: inner_body,
        wrapper_end: if no_wrapping { None } else { Some(closing) },
    })
}

/// Generates the JSON documentation string from the OpenAPI specification.
///
/// # Arguments
/// * `args` - Arguments specifying the input OpenAPI spec and formatting flags.
///
/// # Returns
/// A pretty-printed JSON array containing the [`DocsJsonOutput`].
///
/// # Errors
/// Returns [`AppError`] if the specification cannot be read or parsed.
pub fn generate_docs_json_str(args: &ToDocsJsonArgs) -> AppResult<String> {
    let spec_content = read_spec_source(Path::new(&args.input))?;
    let doc = parse_openapi_document(&spec_content)?;

    let mut base_url = "https://api.example.com".to_string();
    if let Some(first_server) = doc.info.servers.first() {
        base_url = first_server.url.trim_end_matches('/').to_string();
    }

    let mut endpoints: BTreeMap<String, BTreeMap<String, OperationSnippets>> = BTreeMap::new();
    let mut webhooks: BTreeMap<String, BTreeMap<String, OperationSnippets>> = BTreeMap::new();

    for route in &doc.routes {
        let method_key = route.method.to_lowercase();

        let mut enriched_route = route.clone();
        if enriched_route.base_path.is_none() {
            enriched_route.base_path = Some(base_url.clone());
        }

        let default_code = build_strategy_snippet(
            &enriched_route,
            &ReqwestStrategy,
            "#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {",
            args.no_imports,
            args.no_wrapping,
        )?;
        let actix_code = build_strategy_snippet(
            &enriched_route,
            &ActixStrategy,
            "",
            args.no_imports,
            args.no_wrapping,
        )?;
        let axum_code = build_strategy_snippet(
            &enriched_route,
            &AxumStrategy,
            "",
            args.no_imports,
            args.no_wrapping,
        )?;

        let snippets = OperationSnippets {
            default: default_code,
            actix: actix_code,
            axum: axum_code,
        };

        match route.kind {
            RouteKind::Path => {
                endpoints
                    .entry(route.path.clone())
                    .or_default()
                    .insert(method_key, snippets);
            }
            RouteKind::Webhook => {
                webhooks
                    .entry(route.path.clone())
                    .or_default()
                    .insert(method_key, snippets);
            }
        }
    }

    let output = DocsJsonOutput {
        endpoints,
        webhooks,
    };

    let wrapper: Vec<Value> = vec![serde_json::to_value(&output)?];
    Ok(serde_json::to_string_pretty(&wrapper)?)
}

/// Configuration for `to_docs_json` programmatic API.
#[derive(Debug, Default, Clone)]
pub struct ToDocsJsonConfig {
    /// Input OpenAPI file path or URL.
    pub input: String,
    /// Omit imports.
    pub no_imports: bool,
    /// Omit wrapping code.
    pub no_wrapping: bool,
    /// Optional output path.
    pub output: Option<String>,
}

/// Generate documentation JSON from an OpenAPI specification.
///
/// # Arguments
/// * `config` - Programmatic configuration for `to_docs_json`.
///
/// # Returns
/// `Ok(())` on success.
///
/// # Errors
/// Returns [`AppError`] if generation fails.
pub fn to_docs_json(config: &ToDocsJsonConfig) -> AppResult<()> {
    let args = ToDocsJsonArgs {
        input: config.input.clone(),
        no_imports: config.no_imports,
        no_wrapping: config.no_wrapping,
        output: config.output.clone(),
    };
    run_to_docs_json(&args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_generate_docs_json_str() {
        let yaml = r#"
openapi: 3.1.0
info:
  title: Test Docs
  version: 1.0.0
servers:
  - url: https://test.api.com/v1
paths:
  /pets/{id}:
    post:
      operationId: update_pet
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
        - name: filter
          in: query
          required: false
          schema:
            type: string
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
      responses:
        '200':
          description: OK
webhooks:
  newPet:
    post:
      operationId: new_pet_webhook
      requestBody:
        content:
          text/plain:
            schema:
              type: string
      servers:
        - url: https://hooks.api.com
      responses:
        '200':
          description: OK
"#;
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        write!(temp_file, "{}", yaml).expect("Failed to write to temp file");

        let args = ToDocsJsonArgs {
            input: temp_file.path().to_string_lossy().to_string(),
            no_imports: false,
            no_wrapping: false,
            output: None,
        };

        let result = generate_docs_json_str(&args).expect("Failed to generate docs JSON");
        assert!(result.contains(r#""/pets/{id}""#));
        assert!(result.contains(r#""newPet""#));
        assert!(result.contains("reqwest::Client"));
        assert!(result.contains("actix_web"));
        assert!(result.contains("axum"));

        let args_stripped = ToDocsJsonArgs {
            input: temp_file.path().to_string_lossy().to_string(),
            no_imports: true,
            no_wrapping: true,
            output: None,
        };

        let result_stripped =
            generate_docs_json_str(&args_stripped).expect("Failed to generate stripped docs JSON");
        assert!(!result_stripped.contains(r#""imports""#));
        assert!(!result_stripped.contains(r#""wrapper_start""#));
    }

    #[test]
    fn test_run_to_docs_json_stdout_and_file() {
        let yaml = r#"
openapi: 3.0.0
info:
  title: Simple
  version: 1.0.0
paths:
  /ping:
    get:
      responses:
        '200':
          description: pong
"#;
        let mut temp_in = NamedTempFile::new().expect("Failed to create temp file");
        write!(temp_in, "{}", yaml).expect("Failed to write to temp file");
        let temp_out = NamedTempFile::new().expect("Failed to create temp file");

        let config_file = ToDocsJsonConfig {
            input: temp_in.path().to_string_lossy().to_string(),
            no_imports: false,
            no_wrapping: false,
            output: Some(temp_out.path().to_string_lossy().to_string()),
        };
        assert!(to_docs_json(&config_file).is_ok());
        let written = fs::read_to_string(temp_out.path()).expect("Failed to read output");
        assert!(written.contains(r#""/ping""#));

        let config_stdout = ToDocsJsonConfig {
            input: temp_in.path().to_string_lossy().to_string(),
            no_imports: true,
            no_wrapping: true,
            output: None,
        };
        assert!(to_docs_json(&config_stdout).is_ok());
    }

    #[test]
    fn test_run_to_docs_json_errors() {
        let bad_in = ToDocsJsonArgs {
            input: "/nonexistent_file.yaml".to_string(),
            no_imports: false,
            no_wrapping: false,
            output: None,
        };
        assert!(run_to_docs_json(&bad_in).is_err());

        let yaml = "openapi: 3.0.0
info:
  title: T
  version: 1.0
paths: {}
";
        let mut temp_in = NamedTempFile::new().expect("Failed to create temp file");
        write!(temp_in, "{}", yaml).expect("Failed to write to temp file");

        let bad_out = ToDocsJsonArgs {
            input: temp_in.path().to_string_lossy().to_string(),
            no_imports: false,
            no_wrapping: false,
            output: Some("/nonexistent_dir_xyz/output.json".to_string()),
        };
        assert!(run_to_docs_json(&bad_out).is_err());

        let mut bad_yaml = NamedTempFile::new().expect("Failed to create temp file");
        write!(bad_yaml, "not_openapi: [").expect("Failed to write");
        let bad_parse = ToDocsJsonArgs {
            input: bad_yaml.path().to_string_lossy().to_string(),
            no_imports: false,
            no_wrapping: false,
            output: None,
        };
        assert!(run_to_docs_json(&bad_parse).is_err());
    }
}
