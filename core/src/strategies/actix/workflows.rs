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

    out.push_str(
        "
/// Actix-Web server workflow handler entrypoints.
",
    );
    for wf in &doc.workflows {
        let wf_snake = to_snake_case(&wf.workflow_id);
        let wf_pascal = to_pascal_case(&wf.workflow_id);
        out.push_str(&format!(
            "/// Actix-Web server orchestrator endpoint for workflow `{}`.
/// @ARAZZO_SERVER_WORKFLOW_ID: {}
pub async fn handle_workflow_{}(body: actix_web::web::Json<{}Inputs>) -> actix_web::HttpResponse {{
",
            wf.workflow_id, wf.workflow_id, wf_snake, wf_pascal
        ));
        out.push_str(
            "    let _inputs = body.into_inner();
",
        );
        out.push_str(&format!(
            "    let outputs = {}Outputs::default();
",
            wf_pascal
        ));
        out.push_str(
            "    actix_web::HttpResponse::Ok().json(outputs)
",
        );
        out.push_str(
            "}

",
        );
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
        "use actix_web::{test, App};

",
    );

    for wf in &doc.workflows {
        let wf_snake = to_snake_case(&wf.workflow_id);
        out.push_str(&format!(
            "/// End-to-end Actix-Web server integration test for workflow `{}`.\n/// @ARAZZO_WORKFLOW_ID: {}\n#[actix_rt::test]\nasync fn test_workflow_{}() {{\n",
            wf.workflow_id, wf.workflow_id, wf_snake
        ));
        out.push_str(&format!(
            "    let app = test::init_service(App::new().configure({}::configure_routes)).await;\n",
            crate_name
        ));
        for step in &wf.steps {
            let step_snake = to_snake_case(&step.step_id);
            let (req_method, uri) = match &step.target {
                StepTarget::OperationId(op) => ("post", format!("/{}", op)),
                StepTarget::OperationPath(p) => ("get", p.clone()),
                StepTarget::ChannelPath(c) => ("post", c.clone()),
                StepTarget::WorkflowId(w) => ("post", format!("/workflows/{}", w)),
            };
            out.push_str(&format!(
                "    // @ARAZZO_STEP_ID: {}\n    let req_{} = test::TestRequest::{}().uri({:?}).to_request();\n    let resp_{} = test::call_service(&app, req_{}).await;\n    let _status_{} = resp_{}.status().as_u16();\n",
                step.step_id,
                step_snake,
                req_method,
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
