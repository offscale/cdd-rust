#![deny(missing_docs)]

//! Documentation JSON generation module.
//!
//! Generates sample code snippets for OpenAPI operations by delegating directly
//! to the `cdd_core::strategies` and `cdd_core::functions::emit` implementations
//! so documentation examples stay in sync with generated code.

use crate::from_openapi::read_spec_source;
use cdd_core::arazzo::models::{ArazzoDocument, Workflow};
use cdd_core::arazzo::parse::document::{parse_arazzo_document, to_pascal_case};
use cdd_core::error::{AppError, AppResult};
use cdd_core::functions::emit::{generate_function, generate_query_struct};
use cdd_core::openapi::parse::document::parse_openapi_document;
use cdd_core::openapi::parse::models::{ParamSource, ParsedRoute, RouteKind, RustType};
use cdd_core::openapi::parse::routes::naming::to_snake_case;
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
    /// Path or URL to the OpenAPI or Arazzo specification.
    #[clap(short = 'i', long, env = "CDD_INPUT")]
    pub input: String,

    /// Omit import statements from generated code snippets.
    #[clap(long, default_value_t = false, env = "CDD_NO_IMPORTS")]
    pub no_imports: bool,

    /// Omit wrapping boilerplate (`fn main` / function signature) from generated code snippets.
    #[clap(long, default_value_t = false, env = "CDD_NO_WRAPPING")]
    pub no_wrapping: bool,

    /// Optional output file or directory path (writes to stdout if omitted).
    #[clap(short = 'o', long, env = "CDD_OUTPUT")]
    pub output: Option<String>,
}

/// Output structure containing endpoints, webhooks, and workflows for documentation generation.
#[derive(Serialize, Debug)]
pub struct DocsJsonOutput {
    /// Map of paths to HTTP methods and operation snippets.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub endpoints: BTreeMap<String, BTreeMap<String, OperationSnippets>>,
    /// Map of webhook names to HTTP methods and operation snippets.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub webhooks: BTreeMap<String, BTreeMap<String, OperationSnippets>>,
    /// Map of Arazzo `workflowId`s to workflow execution snippets.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub workflows: BTreeMap<String, OperationSnippets>,
}

/// Code snippets for a single operation across supported frameworks.
#[derive(Serialize, Debug)]
pub struct OperationSnippets {
    /// Reqwest SDK client snippet.
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
    /// Core logic / invocation snippet (indented when wrapped, dedented when `no_wrapping` is set).
    pub snippet: String,
    /// Closing boilerplate (`None` when `--no-wrapping` is set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapper_end: Option<String>,
    /// Complete assembled code block combining the active sections.
    pub code: String,
}

/// Assembles a complete [`CodeComponents`] struct from its optional imports,
/// optional wrapper lines, and unindented body lines.
///
/// # Arguments
/// * `raw_imports` - Input `raw_imports` parameter.
/// * `wrapper_start_str` - Input `wrapper_start_str` parameter.
/// * `unindented_lines` - Input `unindented_lines` parameter.
/// * `wrapper_end_str` - Input `wrapper_end_str` parameter.
/// * `no_imports` - Input `no_imports` parameter.
/// * `no_wrapping` - Input `no_wrapping` parameter.
///
/// # Returns
/// Resulting `CodeComponents` value.
fn assemble_code_components(
    raw_imports: String,
    wrapper_start_str: String,
    unindented_lines: &[String],
    wrapper_end_str: String,
    no_imports: bool,
    no_wrapping: bool,
) -> CodeComponents {
    let indent = if no_wrapping { "" } else { "    " };
    let snippet = unindented_lines
        .iter()
        .map(|line| format!("{}{}", indent, line))
        .collect::<Vec<_>>()
        .join("\n");

    let imports = if no_imports { None } else { Some(raw_imports) };
    let wrapper_start = if no_wrapping {
        None
    } else {
        Some(wrapper_start_str)
    };
    let wrapper_end = if no_wrapping {
        None
    } else {
        Some(wrapper_end_str)
    };

    let mut full_parts = Vec::new();
    if let Some(ref imp) = imports {
        full_parts.push(imp.clone());
        full_parts.push(String::new());
    }
    if let Some(ref ws) = wrapper_start {
        full_parts.push(ws.clone());
    }
    full_parts.push(snippet.clone());
    if let Some(ref we) = wrapper_end {
        full_parts.push(we.clone());
    }

    CodeComponents {
        imports,
        wrapper_start,
        snippet,
        wrapper_end,
        code: full_parts.join("\n"),
    }
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
        let p = Path::new(out_path);
        let target = if p.is_dir() {
            p.join("docs.json")
        } else {
            p.to_path_buf()
        };
        fs::write(&target, json_str).map_err(|e| {
            AppError::General(format!("Failed to write to {}: {}", target.display(), e))
        })?;
    } else {
        println!("{}", json_str);
    }
    Ok(())
}

/// Builds a concise SDK primitive invocation snippet for `default` (`ReqwestStrategy`).
///
/// # Arguments
/// * `route` - Input `route` parameter.
/// * `base_url` - Input `base_url` parameter.
/// * `no_imports` - Input `no_imports` parameter.
/// * `no_wrapping` - Input `no_wrapping` parameter.
///
/// # Returns
/// Resulting `CodeComponents` value.
fn build_sdk_client_snippet(
    route: &ParsedRoute,
    base_url: &str,
    no_imports: bool,
    no_wrapping: bool,
) -> CodeComponents {
    let raw_imports = format!(
        "{}\nuse api_client::handlers::*;",
        ReqwestStrategy.handler_imports().trim()
    );
    let wrapper_start_str = "#[tokio::main]\nasync fn main() -> Result<(), Box<dyn std::error::Error>> {\n    let client = reqwest::Client::new();"
        .to_string();
    let wrapper_end_str = "    Ok(())\n}".to_string();

    let tag = route
        .tags
        .first()
        .cloned()
        .unwrap_or_else(|| "default".to_string());
    let module_name = to_snake_case(&tag);
    let handler_name = &route.handler_name;

    let mut lines = Vec::new();
    lines.push(format!("let base_url = {:?};", base_url));
    let mut call_args = vec!["&client".to_string(), "base_url".to_string()];

    // 1. Path parameters
    for param in route
        .params
        .iter()
        .filter(|p| p.source == ParamSource::Path)
    {
        let var_name = to_snake_case(&param.name);
        let parsed_ty = param.rust_type();
        if parsed_ty == RustType::String {
            lines.push(format!(
                "let {}: String = {:?}.to_string();",
                var_name, param.name
            ));
        } else if parsed_ty.is_integer() {
            lines.push(format!("let {}: {} = 1;", var_name, param.ty));
        } else {
            lines.push(format!(
                "let {}: {} = Default::default();",
                var_name, param.ty
            ));
        }
        call_args.push(var_name);
    }

    // 2. Query parameters
    if let Some(qs_param) = route
        .params
        .iter()
        .find(|p| p.source == ParamSource::QueryString)
    {
        let var_name = to_snake_case(&qs_param.name);
        lines.push(format!(
            "let {}: {} = Default::default();",
            var_name, qs_param.ty
        ));
        call_args.push(var_name);
    } else if route.params.iter().any(|p| p.source == ParamSource::Query) {
        let struct_name = format!("{}Query", to_pascal_case(handler_name));
        lines.push(format!(
            "let query = api_client::handlers::{}::{}::default();",
            module_name, struct_name
        ));
        call_args.push("query".to_string());
    }

    // 3. Header parameters
    for param in route
        .params
        .iter()
        .filter(|p| p.source == ParamSource::Header)
    {
        let var_name = to_snake_case(&param.name);
        lines.push(format!("let {} = Default::default();", var_name));
        call_args.push(var_name);
    }

    // 4. Cookie parameters
    for param in route
        .params
        .iter()
        .filter(|p| p.source == ParamSource::Cookie)
    {
        let var_name = to_snake_case(&param.name);
        lines.push(format!("let {} = Default::default();", var_name));
        call_args.push(var_name);
    }

    // 5. Security
    if route.security.iter().any(|g| !g.is_anonymous()) {
        lines.push("let auth_token = Some(\"YOUR_TOKEN\");".to_string());
        call_args.push("auth_token".to_string());
    }

    // 6. Request body
    if let Some(def) = &route.request_body {
        if def.required {
            lines.push("let body = Default::default();".to_string());
        } else {
            lines.push("let body = None;".to_string());
        }
        call_args.push("body".to_string());
    }

    lines.push(format!(
        "let response = api_client::handlers::{}::{}({}).await?;",
        module_name,
        handler_name,
        call_args.join(", ")
    ));
    lines.push("println!(\"{:?}\", response);".to_string());

    assemble_code_components(
        raw_imports,
        wrapper_start_str,
        &lines,
        wrapper_end_str,
        no_imports,
        no_wrapping,
    )
}

/// Builds structured [`CodeComponents`] (`imports`, `wrapper_start`, `snippet`, `wrapper_end`, `code`)
/// for a route using `generate_query_struct` and `generate_function`.
///
/// # Arguments
/// * `route` - Input `route` parameter.
/// * `strategy` - Input `strategy` parameter.
/// * `default_wrapper_start` - Input `default_wrapper_start` parameter.
/// * `no_imports` - Input `no_imports` parameter.
/// * `no_wrapping` - Input `no_wrapping` parameter.
///
/// # Returns
/// Resulting `AppResult<CodeComponents>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
fn build_strategy_snippet(
    route: &ParsedRoute,
    strategy: &impl BackendStrategy,
    default_wrapper_start: &str,
    no_imports: bool,
    no_wrapping: bool,
) -> AppResult<CodeComponents> {
    let raw_imports = strategy.handler_imports().trim().to_string();
    let mut handler_parts = Vec::new();
    if let Some(q) = generate_query_struct(route) {
        handler_parts.push(q.code.trim().to_string());
    }
    handler_parts.push(generate_function(route, strategy)?.trim().to_string());
    let handler_part = handler_parts.join("\n\n");

    let open_brace = handler_part.find('{').unwrap_or(0);
    let sig_wrapper = handler_part[..=open_brace].trim().to_string();
    let after_brace = &handler_part[open_brace + 1..];
    let inner_body = after_brace
        .trim_end()
        .strip_suffix('}')
        .unwrap_or(after_brace)
        .trim();
    let inner_lines: Vec<String> = inner_body
        .lines()
        .map(|l| l.strip_prefix("    ").unwrap_or(l).to_string())
        .collect();
    let closing = "}".to_string();

    let wrapper_start_str = [default_wrapper_start, sig_wrapper.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(assemble_code_components(
        raw_imports,
        wrapper_start_str,
        &inner_lines,
        closing,
        no_imports,
        no_wrapping,
    ))
}

/// Builds workflow code snippets (`default`, `actix`, `axum`) for a single Arazzo [`Workflow`].
///
/// # Arguments
/// * `wf` - Input `wf` parameter.
/// * `no_imports` - Input `no_imports` parameter.
/// * `no_wrapping` - Input `no_wrapping` parameter.
///
/// # Returns
/// Resulting `OperationSnippets` value.
fn build_workflow_snippets(
    wf: &Workflow,
    no_imports: bool,
    no_wrapping: bool,
) -> OperationSnippets {
    let wf_snake = to_snake_case(&wf.workflow_id);
    let wf_pascal = to_pascal_case(&wf.workflow_id);

    let default_lines = vec![
        format!("let inputs = {}Inputs::default();", wf_pascal),
        format!("let outputs = client.{}(inputs).await?;", wf_snake),
        "println!(\"{:?}\", outputs);".to_string(),
    ];
    let default = assemble_code_components(
        format!(
            "use reqwest::Client;\nuse api_client::workflows::{{WorkflowsClient, {}Inputs}};",
            wf_pascal
        ),
        "#[tokio::main]\nasync fn main() -> Result<(), Box<dyn std::error::Error>> {\n    let client = WorkflowsClient::new(Client::new());"
            .to_string(),
        &default_lines,
        "    Ok(())\n}".to_string(),
        no_imports,
        no_wrapping,
    );

    let actix_lines = vec![
        "let client = WorkflowsClient::new(reqwest::Client::new());".to_string(),
        format!(
            "let outputs = client.{}(body.into_inner()).await.unwrap_or_default();",
            wf_snake
        ),
        "HttpResponse::Ok().json(outputs)".to_string(),
    ];
    let actix = assemble_code_components(
        format!(
            "use actix_web::{{web, HttpResponse}};\nuse crate::workflows::{{WorkflowsClient, {0}Inputs}};",
            wf_pascal
        ),
        format!(
            "pub async fn handle_workflow_{}(body: web::Json<{}Inputs>) -> HttpResponse {{",
            wf_snake, wf_pascal
        ),
        &actix_lines,
        "}".to_string(),
        no_imports,
        no_wrapping,
    );

    let axum_lines = vec![
        "let client = WorkflowsClient::new(reqwest::Client::new());".to_string(),
        format!(
            "let outputs = client.{}(inputs).await.unwrap_or_default();",
            wf_snake
        ),
        "Json(outputs)".to_string(),
    ];
    let axum = assemble_code_components(
        format!(
            "use axum::Json;\nuse crate::workflows::{{WorkflowsClient, {0}Inputs, {0}Outputs}};",
            wf_pascal
        ),
        format!(
            "pub async fn handle_workflow_{0}(Json(inputs): Json<{1}Inputs>) -> Json<{1}Outputs> {{",
            wf_snake, wf_pascal
        ),
        &axum_lines,
        "}".to_string(),
        no_imports,
        no_wrapping,
    );

    OperationSnippets {
        default,
        actix,
        axum,
    }
}

/// Generates the JSON documentation string from an Arazzo 1.1.0 specification.
///
/// # Arguments
/// * `doc` - Input `doc` parameter.
/// * `args` - Input `args` parameter.
///
/// # Returns
/// Resulting `AppResult<String>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
fn generate_arazzo_docs_json_str(doc: &ArazzoDocument, args: &ToDocsJsonArgs) -> AppResult<String> {
    let mut workflows = BTreeMap::new();
    for wf in &doc.workflows {
        workflows.insert(
            wf.workflow_id.clone(),
            build_workflow_snippets(wf, args.no_imports, args.no_wrapping),
        );
    }

    let output = DocsJsonOutput {
        endpoints: BTreeMap::new(),
        webhooks: BTreeMap::new(),
        workflows,
    };

    let wrapper: Vec<Value> = vec![serde_json::to_value(&output)?];
    Ok(serde_json::to_string_pretty(&wrapper)?)
}

/// Generates the JSON documentation string from the OpenAPI or Arazzo specification.
///
/// # Arguments
/// * `args` - Arguments specifying the input specification and formatting flags.
///
/// # Returns
/// A pretty-printed JSON array containing the [`DocsJsonOutput`].
///
/// # Errors
/// Returns [`AppError`] if the specification cannot be read or parsed.
pub fn generate_docs_json_str(args: &ToDocsJsonArgs) -> AppResult<String> {
    let spec_content = read_spec_source(Path::new(&args.input))?;
    if let Ok(arazzo_doc) = parse_arazzo_document(&spec_content) {
        return generate_arazzo_docs_json_str(&arazzo_doc, args);
    }
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
        let route_base_url = enriched_route
            .base_path
            .clone()
            .unwrap_or_else(|| base_url.clone());
        if enriched_route.base_path.is_none() {
            enriched_route.base_path = Some(route_base_url.clone());
        }

        let default_code = build_sdk_client_snippet(
            &enriched_route,
            &route_base_url,
            args.no_imports,
            args.no_wrapping,
        );
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
        workflows: BTreeMap::new(),
    };

    let wrapper: Vec<Value> = vec![serde_json::to_value(&output)?];
    Ok(serde_json::to_string_pretty(&wrapper)?)
}

/// Configuration for `to_docs_json` programmatic API.
#[derive(Debug, Default, Clone)]
pub struct ToDocsJsonConfig {
    /// Input OpenAPI or Arazzo file path or URL.
    pub input: String,
    /// Omit imports.
    pub no_imports: bool,
    /// Omit wrapping code.
    pub no_wrapping: bool,
    /// Optional output path.
    pub output: Option<String>,
}

/// Generate documentation JSON from an OpenAPI or Arazzo specification.
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
    use tempfile::{tempdir, NamedTempFile};

    #[test]
    fn test_generate_docs_json_str() {
        let yaml = r#"
openapi: 3.2.0
info:
  title: Test Docs
  version: 1.0.0
servers:
  - url: https://test.api.com/v1
components:
  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
paths:
  /pets/{id}/{count}/{uuid}:
    post:
      operationId: update_pet
      tags:
        - pets
      security:
        - bearerAuth: []
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
        - name: count
          in: path
          required: true
          schema:
            type: integer
        - name: uuid
          in: path
          required: true
          schema:
            type: string
            format: uuid
        - name: filter
          in: query
          required: false
          schema:
            type: string
        - name: X-Header
          in: header
          required: false
          schema:
            type: string
        - name: session_id
          in: cookie
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
  /raw_query:
    get:
      operationId: raw_query_op
      parameters:
        - name: raw_qs
          in: querystring
          content:
            application/x-www-form-urlencoded:
              schema:
                type: string
      responses:
        '200':
          description: OK
webhooks:
  newPet:
    post:
      operationId: new_pet_webhook
      requestBody:
        required: false
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
        assert!(result.contains(r#""/pets/{id}/{count}/{uuid}""#));
        assert!(result.contains(r#""newPet""#));
        assert!(result.contains(r#""snippet": "    let base_url = \"https://test.api.com/v1\";"#));
        assert!(result.contains("api_client::handlers::pets::update_pet"));
        assert!(result.contains("api_client::handlers::default::raw_query_op"));
        assert!(result.contains("reqwest::Client::new()"));
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
        assert!(
            result_stripped.contains(r#""snippet": "let base_url = \"https://test.api.com/v1\";"#)
        );
        assert!(result_stripped.contains(r#""code": "let base_url = \"https://test.api.com/v1\";"#));

        // Test Arazzo workflow documentation generation
        let arazzo_yaml = r#"
arazzo: 1.1.0
info:
  title: Pet Purchase Workflow
  version: 1.0.0
sourceDescriptions:
  - name: petstore
    url: ./openapi.yaml
    type: openapi
workflows:
  - workflowId: purchasePet
    summary: Purchase a pet end-to-end
    steps:
      - stepId: findPet
        operationId: findPetsByStatus
"#;
        let mut arazzo_file = NamedTempFile::new().expect("Failed to create arazzo temp file");
        write!(arazzo_file, "{}", arazzo_yaml).expect("Failed to write arazzo temp file");

        let arazzo_args = ToDocsJsonArgs {
            input: arazzo_file.path().to_string_lossy().to_string(),
            no_imports: false,
            no_wrapping: false,
            output: None,
        };
        let arazzo_json =
            generate_docs_json_str(&arazzo_args).expect("Failed to generate Arazzo docs JSON");
        assert!(arazzo_json.contains(r#""workflows""#));
        assert!(arazzo_json.contains(r#""purchasePet""#));
        assert!(arazzo_json.contains("WorkflowsClient::new"));
        assert!(arazzo_json.contains("client.purchase_pet(inputs).await?"));
        assert!(arazzo_json.contains("handle_workflow_purchase_pet"));
        assert!(
            arazzo_json.contains(r#""snippet": "    let inputs = PurchasePetInputs::default();"#)
        );

        let arazzo_stripped = ToDocsJsonArgs {
            input: arazzo_file.path().to_string_lossy().to_string(),
            no_imports: true,
            no_wrapping: true,
            output: None,
        };
        let arazzo_json_stripped = generate_docs_json_str(&arazzo_stripped)
            .expect("Failed to generate stripped Arazzo docs JSON");
        assert!(!arazzo_json_stripped.contains(r#""imports""#));
        assert!(!arazzo_json_stripped.contains(r#""wrapper_start""#));
        assert!(arazzo_json_stripped
            .contains(r#""snippet": "let inputs = PurchasePetInputs::default();"#));
        assert!(
            arazzo_json_stripped.contains(r#""code": "let inputs = PurchasePetInputs::default();"#)
        );
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
        assert!(written.contains("api_client::handlers::default::get_ping"));

        // Verify directory output writes <dir>/docs.json
        let out_dir = tempdir().expect("Failed to create temp output dir");
        let config_dir = ToDocsJsonConfig {
            input: temp_in.path().to_string_lossy().to_string(),
            no_imports: false,
            no_wrapping: false,
            output: Some(out_dir.path().to_string_lossy().to_string()),
        };
        assert!(to_docs_json(&config_dir).is_ok());
        let dir_written =
            fs::read_to_string(out_dir.path().join("docs.json")).expect("Failed to read docs.json");
        assert!(dir_written.contains(r#""/ping""#));

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
