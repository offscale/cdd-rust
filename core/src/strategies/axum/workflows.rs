#![deny(missing_docs)]

//! # Axum Server Workflow Orchestrator & E2E Test Generation
//!
//! Generates stateful server-side workflow handlers and multi-step `tower::ServiceExt::oneshot`
//! integration test suites from an [`ArazzoDocument`].

use crate::arazzo::models::{ArazzoDocument, StepTarget};
use crate::arazzo::parse::document::to_pascal_case;
use crate::error::AppResult;
use crate::openapi::parse::document::ParsedOpenApi;
use crate::openapi::parse::routes::naming::to_snake_case;
use std::collections::BTreeMap;

/// Emits strongly typed workflow input and output models for Axum server targets.
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

/// Emits server-side Axum workflow orchestrator handlers and models.
///
/// # Arguments
/// * `doc` - The parsed [`ArazzoDocument`].
/// * `linked_specs` - Map of linked [`ParsedOpenApi`] specifications.
///
/// # Returns
/// Valid Rust source code containing the workflow models and Axum orchestrator handlers.
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
/// Axum server workflow handler entrypoints.
",
    );
    for wf in &doc.workflows {
        let wf_snake = to_snake_case(&wf.workflow_id);
        let wf_pascal = to_pascal_case(&wf.workflow_id);
        out.push_str(&format!(
            "/// Axum server orchestrator endpoint for workflow `{}`.
/// @ARAZZO_SERVER_WORKFLOW_ID: {}
pub async fn handle_workflow_{}(axum::Json(_inputs): axum::Json<{}Inputs>) -> axum::Json<{}Outputs> {{
",
            wf.workflow_id, wf.workflow_id, wf_snake, wf_pascal, wf_pascal
        ));
        out.push_str(&format!(
            "    axum::Json({}Outputs::default())
",
            wf_pascal
        ));
        out.push_str(
            "}

",
        );
    }

    Ok(out)
}

/// Emits multi-step `#[tokio::test]` E2E integration tests executing each workflow's steps
/// against an in-memory Axum router using `tower::ServiceExt::oneshot`.
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
    let mut out = String::new();
    out.push_str(
        "#![allow(missing_docs)]

",
    );
    let canonical_json =
        serde_json::to_string(&crate::arazzo::emit::generate_arazzo_document(doc)?)?;
    out.push_str(&format!(
        "/// @ARAZZO_SPEC_JSON: {}
",
        canonical_json
    ));
    out.push_str(
        "use axum::body::Body;
",
    );
    out.push_str(
        "use axum::http::Request;
",
    );
    out.push_str(
        "use tower::ServiceExt;

",
    );

    for wf in &doc.workflows {
        let wf_snake = to_snake_case(&wf.workflow_id);
        out.push_str(&format!(
            "/// End-to-end Axum server integration test for workflow `{}`.\n/// @ARAZZO_WORKFLOW_ID: {}\n#[tokio::test]\nasync fn test_workflow_{}() {{\n",
            wf.workflow_id, wf.workflow_id, wf_snake
        ));
        out.push_str(&format!("    let app = {}::create_router();\n", crate_name));
        for step in &wf.steps {
            let step_snake = to_snake_case(&step.step_id);
            let (method, uri) = match &step.target {
                StepTarget::OperationId(op) => ("POST", format!("/{}", op)),
                StepTarget::OperationPath(p) => ("GET", p.clone()),
                StepTarget::ChannelPath(c) => ("POST", c.clone()),
                StepTarget::WorkflowId(w) => ("POST", format!("/workflows/{}", w)),
            };
            out.push_str(&format!(
                r#"    // @ARAZZO_STEP_ID: {}
    let req_{} = Request::builder().method({:?}).uri({:?}).body(Body::empty()).expect("request");
    let resp_{} = app.clone().oneshot(req_{}).await.expect("response");
    let _status_{} = resp_{}.status().as_u16();
"#,
                step.step_id,
                step_snake,
                method,
                uri,
                step_snake,
                step_snake,
                step_snake,
                step_snake
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
    fn test_emit_axum_workflow_module_and_e2e_tests_valid_ast() {
        let yaml = r##"
arazzo: 1.1.0
info:
  title: Axum Workflows
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

        let tests = emit_workflow_tests(&doc, "my_axum_app").expect("tests");
        assert!(tests.contains("oneshot"));
        assert!(SourceFile::parse(&tests, Edition::Edition2021)
            .errors()
            .is_empty());
    }
}
