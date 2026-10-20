#![deny(missing_docs)]

//! # Actix-Web Server Workflow Orchestrator & E2E Test Generation
//!
//! Generates stateful server-side workflow handlers and multi-step `actix_web::test`
//! integration test suites from an [`ArazzoDocument`].

use crate::arazzo::models::{ArazzoDocument, StepTarget};
use crate::arazzo::parse::document::to_pascal_case;
use crate::error::AppResult;
use crate::openapi::parse::document::ParsedOpenApi;
use crate::openapi::parse::routes::naming::to_snake_case;
use std::collections::BTreeMap;

/// Emits strongly typed workflow input and output models for Actix-Web server targets.
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

/// Emits server-side Actix-Web workflow orchestrator handlers and models.
///
/// # Arguments
/// * `doc` - The parsed [`ArazzoDocument`].
/// * `linked_specs` - Map of linked [`ParsedOpenApi`] specifications.
///
/// # Returns
/// Valid Rust source code containing the workflow models and Actix-Web orchestrator handlers.
///
/// # Errors
/// Returns [`crate::error::AppError`] if emission fails.
pub fn emit_workflow_module(
    doc: &ArazzoDocument,
    linked_specs: &BTreeMap<String, ParsedOpenApi>,
) -> AppResult<String> {
    let mut out = crate::strategies::reqwest::workflows::emit_workflow_module(doc, linked_specs)?;

    out.push_str("\n/// Actix-Web server workflow handler entrypoints.\n");
    for wf in &doc.workflows {
        let wf_snake = to_snake_case(&wf.workflow_id);
        let wf_pascal = to_pascal_case(&wf.workflow_id);
        out.push_str(&format!(
            "/// Actix-Web server orchestrator endpoint for workflow `{}`.\n/// @ARAZZO_WORKFLOW_ID: {}\npub async fn handle_workflow_{}(body: actix_web::web::Json<{}Inputs>) -> actix_web::HttpResponse {{\n",
            wf.workflow_id, wf.workflow_id, wf_snake, wf_pascal
        ));
        out.push_str("    let inputs = body.into_inner();\n");
        out.push_str("    let client = WorkflowsClient::new(reqwest::Client::new());\n");
        out.push_str(&format!(
            "    let outputs = client.{}(inputs).await.unwrap_or_default();\n",
            wf_snake
        ));
        out.push_str("    actix_web::HttpResponse::Ok().json(outputs)\n}\n\n");
    }

    Ok(out)
}

/// Emits multi-step `#[actix_rt::test]` E2E integration tests executing each workflow's steps
/// against an in-memory Actix-Web service router.
///
/// # Arguments
/// * `doc` - The parsed [`ArazzoDocument`].
/// * `crate_name` - Name of the generated server crate.
///
/// # Returns
/// Valid Rust integration test module source code.
///
/// # Errors
/// Returns [`crate::error::AppError`] if emission fails.
pub fn emit_workflow_tests(doc: &ArazzoDocument, crate_name: &str) -> AppResult<String> {
    crate::arazzo::parse::validate_arazzo_document(doc)?;
    let mut out = String::new();
    out.push_str("#![allow(missing_docs)]\n\n");
    crate::strategies::reqwest::workflows::emit_arazzo_header_comments(&mut out, doc);
    out.push_str("use actix_web::{test, App};\nuse serde::{Deserialize, Serialize};\n\n");

    let models_src = emit_workflow_models(doc, &BTreeMap::new())?;
    for line in models_src.lines() {
        if line.starts_with("#![") || line.starts_with("//!") || line.starts_with("use ") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    for wf in &doc.workflows {
        let wf_snake = to_snake_case(&wf.workflow_id);
        crate::strategies::reqwest::workflows::emit_workflow_doc_annotations(&mut out, wf, "");
        out.push_str(&format!(
            "#[actix_rt::test]\nasync fn test_workflow_{}() {{\n",
            wf_snake
        ));
        out.push_str(&format!(
            "    let app = test::init_service(App::new().configure({}::configure_routes)).await;\n",
            crate_name
        ));
        for step in &wf.steps {
            let step_snake = to_snake_case(&step.step_id);
            crate::strategies::reqwest::workflows::emit_step_doc_annotations(
                &mut out, step, "    ",
            );
            let (req_method, uri) = match &step.target {
                StepTarget::OperationId(op) => ("post", format!("/{}", op)),
                StepTarget::OperationPath(p) => ("get", p.clone()),
                StepTarget::ChannelPath(c) => ("post", c.clone()),
                StepTarget::WorkflowId(w) => ("post", format!("/workflows/{}", w)),
            };
            for crit in &step.success_criteria {
                out.push_str(&format!(
                    "    // @ARAZZO_CRITERION: condition={}\n",
                    crit.condition
                ));
            }
            for (out_key, out_expr) in &step.outputs {
                out.push_str(&format!(
                    "    // @ARAZZO_STEP_OUTPUT: {}={}\n",
                    out_key,
                    crate::strategies::reqwest::workflows::format_expr_or_selector(out_expr)
                ));
            }
            out.push_str(&format!(
                "    let req_{0} = test::TestRequest::{1}().uri({2:?}).to_request();\n    let resp_{0} = test::call_service(&app, req_{0}).await;\n    assert!(!resp_{0}.status().is_server_error());\n",
                step_snake, req_method, uri
            ));
        }
        out.push_str("}\n\n");
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
    fn test_emit_actix_workflow_module_and_e2e_tests_valid_ast() {
        let yaml = r##"
arazzo: 1.1.0
info:
  title: Actix Workflows
  version: 1.0.0
sourceDescriptions:
  - name: api
    url: ./openapi.yaml
    type: openapi
workflows:
  - workflowId: userSignup
    steps:
      - stepId: create
        operationId: createUser
      - stepId: fetch
        operationPath: "{$sourceDescriptions.api.url}#/paths/~1users/get"
      - stepId: event
        channelPath: /users/created
      - stepId: sub
        workflowId: userSignup
"##;
        let doc = parse_arazzo_document(yaml).expect("parse");
        let linked = BTreeMap::new();
        let models = emit_workflow_models(&doc, &linked).expect("models");
        assert!(SourceFile::parse(&models, Edition::Edition2021)
            .errors()
            .is_empty());

        let module = emit_workflow_module(&doc, &linked).expect("module");
        assert!(module.contains("handle_workflow_user_signup"));
        assert!(SourceFile::parse(&module, Edition::Edition2021)
            .errors()
            .is_empty());

        let tests = emit_workflow_tests(&doc, "my_actix_app").expect("tests");
        assert!(tests.contains("#[actix_rt::test]"));
        assert!(SourceFile::parse(&tests, Edition::Edition2021)
            .errors()
            .is_empty());
    }
}
