#![deny(missing_docs)]

//! # Clap CLI Workflow Code Generation (Reusing Frontend Client SDK)
//!
//! Generates Clap `WorkflowCommands` subcommands, `<WorkflowId>CliArgs` structs,
//! and CLI execution runners that delegate directly to the generated Reqwest Client SDK
//! (`crate::workflows::WorkflowsClient`), avoiding duplicate HTTP logic.

use crate::arazzo::models::ArazzoDocument;
use crate::arazzo::parse::document::to_pascal_case;
use crate::error::AppResult;
use crate::openapi::parse::document::ParsedOpenApi;
use crate::openapi::parse::routes::naming::to_snake_case;
use std::collections::BTreeMap;

/// Emits strongly typed workflow input and output models by delegating to the Client SDK models emitter.
///
/// # Arguments
/// * `doc` - The parsed [`ArazzoDocument`].
/// * `linked_specs` - Map of linked [`ParsedOpenApi`] specifications.
///
/// # Returns
/// Valid Rust source code defining workflow input and output structs.
///
/// # Errors
/// Returns [`crate::error::AppError`] if emission fails.
pub fn emit_workflow_models(
    doc: &ArazzoDocument,
    linked_specs: &BTreeMap<String, ParsedOpenApi>,
) -> AppResult<String> {
    crate::strategies::reqwest::workflows::emit_workflow_models(doc, linked_specs)
}

/// Emits the Clap CLI workflow module that embeds the Client SDK `WorkflowsClient`
/// and exposes Clap subcommands (`WorkflowCommands`) that reuse the SDK directly.
///
/// # Arguments
/// * `doc` - The parsed [`ArazzoDocument`].
/// * `linked_specs` - Map of linked [`ParsedOpenApi`] specifications.
///
/// # Returns
/// Valid Rust source code containing the SDK `WorkflowsClient` and Clap CLI subcommand runners.
///
/// # Errors
/// Returns [`crate::error::AppError`] if emission fails.
pub fn emit_workflow_module(
    doc: &ArazzoDocument,
    linked_specs: &BTreeMap<String, ParsedOpenApi>,
) -> AppResult<String> {
    let mut out = crate::strategies::reqwest::workflows::emit_workflow_module(doc, linked_specs)?;

    out.push_str(
        "
/// Clap CLI subcommand enum for executing Arazzo 1.1.0 workflows via the Client SDK.
",
    );
    out.push_str(
        "#[derive(clap::Subcommand, Debug, Clone)]
",
    );
    out.push_str(
        "pub enum WorkflowCommands {
",
    );
    for wf in &doc.workflows {
        let wf_pascal = to_pascal_case(&wf.workflow_id);
        let summary = wf.summary.as_deref().unwrap_or("Execute Arazzo workflow.");
        out.push_str(&format!(
            "    /// {}
",
            summary
        ));
        out.push_str(&format!(
            "    {}({}CliArgs),
",
            wf_pascal, wf_pascal
        ));
    }
    out.push_str(
        "}

",
    );

    for wf in &doc.workflows {
        let wf_pascal = to_pascal_case(&wf.workflow_id);
        out.push_str(&format!(
            "/// Clap CLI arguments for workflow `{}`.
/// @ARAZZO_CLI_WORKFLOW_ID: {}
#[derive(clap::Args, Debug, Clone, Default)]
pub struct {}CliArgs {{
",
            wf.workflow_id, wf.workflow_id, wf_pascal
        ));
        if let Some(inp) = &wf.inputs {
            for f in &inp.input_struct.fields {
                let f_desc = f
                    .description
                    .as_deref()
                    .unwrap_or("Workflow CLI input flag.");
                out.push_str(&format!("    /// {}\n", f_desc));
                out.push_str("    #[clap(long)]\n");
                out.push_str(&format!("    pub {}: Option<String>,\n", f.name));
            }
        }
        out.push_str(
            "}

",
        );
    }

    out.push_str("/// Executes a Clap [`WorkflowCommands`] subcommand by delegating to the Client SDK [`WorkflowsClient`].
");
    out.push_str(
        "pub async fn execute_workflow_cli(
",
    );
    out.push_str(
        "    command: WorkflowCommands,
",
    );
    out.push_str(
        "    sdk_client: &WorkflowsClient,
",
    );
    out.push_str(
        ") -> Result<serde_json::Value, WorkflowError> {
",
    );
    out.push_str(
        "    match command {
",
    );
    for wf in &doc.workflows {
        let wf_pascal = to_pascal_case(&wf.workflow_id);
        let wf_snake = to_snake_case(&wf.workflow_id);
        out.push_str(&format!(
            "        WorkflowCommands::{}(cli_args) => {{\n",
            wf_pascal
        ));
        out.push_str(&format!(
            "            let mut inputs = {}Inputs::default();\n",
            wf_pascal
        ));
        if let Some(inp) = &wf.inputs {
            for f in &inp.input_struct.fields {
                out.push_str(&format!(
                    "            if let Some(raw_val) = &cli_args.{f_name} {{\n                if let Ok(parsed) = serde_json::from_str(raw_val) {{\n                    inputs.{f_name} = parsed;\n                }}\n            }}\n",
                    f_name = f.name
                ));
            }
        }
        out.push_str("            let _ = &cli_args;\n");
        out.push_str(&format!(
            "            let res = sdk_client.{}(inputs).await?;\n",
            wf_snake
        ));
        out.push_str("            Ok(serde_json::to_value(res)?)\n");
        out.push_str("        }\n");
    }
    out.push_str(
        "    }
",
    );
    out.push_str(
        "}
",
    );

    Ok(out)
}

/// Emits CLI integration tests for all workflows in `doc` delegating to `execute_workflow_cli`.
///
/// # Arguments
/// * `doc` - The parsed [`ArazzoDocument`].
/// * `crate_name` - Name of the generated crate.
///
/// # Returns
/// Valid Rust test module source code.
///
/// # Errors
/// Returns [`crate::error::AppError`] if emission fails.
pub fn emit_workflow_tests(doc: &ArazzoDocument, crate_name: &str) -> AppResult<String> {
    let mut out = String::new();
    out.push_str(
        "#![allow(missing_docs)]

",
    );
    for wf in &doc.workflows {
        let wf_snake = to_snake_case(&wf.workflow_id);
        let wf_pascal = to_pascal_case(&wf.workflow_id);
        out.push_str(&format!(
            "/// @ARAZZO_CLI_TEST_WORKFLOW_ID: {}
",
            wf.workflow_id
        ));
        out.push_str(
            "#[tokio::test]
",
        );
        out.push_str(&format!(
            "async fn test_cli_workflow_{}() {{
",
            wf_snake
        ));
        out.push_str(
            "    let http_client = reqwest::Client::new();
",
        );
        out.push_str(&format!(
            "    let sdk_client = {}::workflows::WorkflowsClient::new(http_client);
",
            crate_name
        ));
        out.push_str(&format!(
            "    let cmd = {}::workflows::WorkflowCommands::{}({}::workflows::{}CliArgs::default());
",
            crate_name, wf_pascal, crate_name, wf_pascal
        ));
        out.push_str(&format!(
            "    let _ = {}::workflows::execute_workflow_cli(cmd, &sdk_client).await;
",
            crate_name
        ));
        out.push_str(
            "}

",
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arazzo::parse::parse_arazzo_document;
    use ra_ap_edition::Edition;
    use ra_ap_syntax::SourceFile;

    #[test]
    fn test_emit_clap_cli_workflow_reusing_sdk_valid_ast() {
        let yaml = r##"
arazzo: 1.1.0
info:
  title: CLI Workflows
  version: 1.0.0
sourceDescriptions:
  - name: api
    url: http://localhost:8080
    type: openapi
workflows:
  - workflowId: noInputFlow
    steps:
      - stepId: s0
        operationId: ping
  - workflowId: runOrder
    summary: Run order from CLI
    inputs:
      type: object
      properties:
        itemId:
          type: string
          description: Item identifier
    steps:
      - stepId: s1
        operationId: createOrder
"##;
        let doc = parse_arazzo_document(yaml).expect("parse");
        let linked = BTreeMap::new();
        let models_code = emit_workflow_models(&doc, &linked).expect("models");
        assert!(SourceFile::parse(&models_code, Edition::Edition2021)
            .errors()
            .is_empty());

        let mod_code = emit_workflow_module(&doc, &linked).expect("module");
        assert!(mod_code.contains("pub enum WorkflowCommands"));
        assert!(mod_code.contains("pub async fn execute_workflow_cli"));
        assert!(mod_code.contains("sdk_client.run_order(inputs).await"));
        let parsed = SourceFile::parse(&mod_code, Edition::Edition2021);
        assert!(parsed.errors().is_empty());

        let test_code = emit_workflow_tests(&doc, "my_cli").expect("tests");
        let parsed_tests = SourceFile::parse(&test_code, Edition::Edition2021);
        assert!(parsed_tests.errors().is_empty());
    }
}
