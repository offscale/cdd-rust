#![deny(missing_docs)]

//! # Reqwest Client SDK Workflow Code Generation
//!
//! Generates strongly typed workflow input/output models, `WorkflowsClient`
//! orchestrators, multi-step state machines, payload replacements, criteria assertions,
//! retry loops, and mock tests from an [`ArazzoDocument`].

use crate::arazzo::models::{
    ActionOrReusable, ActionTarget, ArazzoDocument, Criterion, CriterionType, FailureAction,
    FailureActionType, ParameterOrReusable, ParameterValue, RuntimeExpressionOrSelector,
    StepTarget, SuccessAction, SuccessActionType, Workflow, WorkflowStep,
};
use crate::arazzo::parse::document::to_pascal_case;
use crate::arazzo::parse::{
    resolve_step_operation, runtime_expression_to_rust, topological_sort_steps,
};
use crate::classes::parse::models::ParsedStruct;
use crate::error::AppResult;
use crate::openapi::parse::document::ParsedOpenApi;
use crate::openapi::parse::routes::naming::to_snake_case;
use std::collections::BTreeMap;

/// Emits strongly typed Rust structs for workflow inputs, step outputs, and workflow outputs.
///
/// # Arguments
/// * `doc` - The parsed [`ArazzoDocument`].
/// * `_linked_specs` - Map of linked [`ParsedOpenApi`] specifications.
///
/// # Returns
/// Valid Rust source code defining all workflow input and output structs.
///
/// # Errors
/// Returns [`crate::error::AppError`] if emission fails.
pub fn emit_workflow_models(
    doc: &ArazzoDocument,
    _linked_specs: &BTreeMap<String, ParsedOpenApi>,
) -> AppResult<String> {
    let mut out = String::new();
    out.push_str(
        "#![deny(missing_docs)]

",
    );
    out.push_str(
        "//! Strongly typed Arazzo 1.1.0 workflow input and output models.

",
    );
    out.push_str(
        "use derive_more::{Display, Error, From};\nuse serde::{Deserialize, Serialize};\n\n",
    );
    out.push_str(
        r#"/// Strongly typed error enum for Arazzo 1.1.0 workflow execution.
#[derive(Debug, Display, Error, From)]
pub enum WorkflowError {
    /// HTTP client transport failure.
    #[display("HTTP error: {_0}")]
    Http(reqwest::Error),
    /// Invalid HTTP method encountered in step configuration.
    #[display("Invalid HTTP method: {_0}")]
    InvalidMethod(reqwest::header::InvalidHeaderValue),
    /// JSON serialization or deserialization failure.
    #[display("JSON error: {_0}")]
    Json(serde_json::Error),
    /// Regular expression compilation failure.
    #[display("Regex error: {_0}")]
    Regex(regex::Error),
    /// Step criterion assertion or runtime execution failure.
    #[from(ignore)]
    #[error(ignore)]
    #[display("Workflow execution error: {_0}")]
    Execution(String),
}

"#,
    );

    if let Some(comps) = &doc.components {
        for (comp_name, schema) in &comps.inputs {
            emit_parsed_schema_struct(
                &mut out,
                &to_pascal_case(comp_name),
                &schema.input_struct,
                Some(comp_name),
            );
        }
    }

    for wf in &doc.workflows {
        let wf_pascal = to_pascal_case(&wf.workflow_id);
        let input_struct_name = format!("{}Inputs", wf_pascal);
        if let Some(inp) = &wf.inputs {
            if let Some(ref_str) = &inp.reference {
                let target_comp = ref_str
                    .rsplit(['/', '.'])
                    .next()
                    .unwrap_or(ref_str.as_str());
                out.push_str(&format!(
                    "/// Inputs for workflow `{}`.\n/// @ARAZZO_INPUTS_REF: {} -> {}\npub type {} = {};\n\n",
                    wf.workflow_id,
                    wf.workflow_id,
                    ref_str,
                    input_struct_name,
                    to_pascal_case(target_comp)
                ));
            } else {
                emit_parsed_schema_struct(&mut out, &input_struct_name, &inp.input_struct, None);
            }
        } else {
            out.push_str(&format!(
                "/// Empty inputs for workflow `{}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct {} {{}}

",
                wf.workflow_id, input_struct_name
            ));
        }

        for step in &wf.steps {
            let step_out_name = format!("{}{}Outputs", wf_pascal, to_pascal_case(&step.step_id));
            out.push_str(&format!(
                "/// Step outputs for step `{}` in workflow `{}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct {} {{
",
                step.step_id, wf.workflow_id, step_out_name
            ));
            for out_key in step.outputs.keys() {
                let field_name = to_snake_case(out_key);
                out.push_str(&format!(
                    "    /// Extracted step output `{}`.
    pub {}: serde_json::Value,
",
                    out_key, field_name
                ));
            }
            out.push_str(
                "}

",
            );
        }

        let wf_out_name = format!("{}Outputs", wf_pascal);
        out.push_str(&format!(
            "/// Final outputs for workflow `{}`.
/// @ARAZZO_OUTPUTS_FOR: {}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct {} {{
",
            wf.workflow_id, wf.workflow_id, wf_out_name
        ));
        for out_key in wf.outputs.keys() {
            let field_name = to_snake_case(out_key);
            out.push_str(&format!(
                "    /// Extracted workflow output `{}`.
    pub {}: serde_json::Value,
",
                out_key, field_name
            ));
        }
        out.push_str(
            "}

",
        );
    }

    Ok(out)
}

/// Emits the Reqwest Client SDK workflow orchestrator module (`WorkflowsClient` + methods).
///
/// # Arguments
/// * `doc` - The parsed [`ArazzoDocument`].
/// * `linked_specs` - Map of linked [`ParsedOpenApi`] specifications.
///
/// # Returns
/// Valid Rust source code containing models and the `WorkflowsClient` implementation.
///
/// # Errors
/// Returns [`crate::error::AppError`] if emission fails.
pub fn emit_workflow_module(
    doc: &ArazzoDocument,
    linked_specs: &BTreeMap<String, ParsedOpenApi>,
) -> AppResult<String> {
    let mut out = emit_workflow_models(doc, linked_specs)?;

    let canonical_json =
        serde_json::to_string(&crate::arazzo::emit::generate_arazzo_document(doc)?)
            .unwrap_or_default();

    out.push_str(&format!("/// @ARAZZO_VERSION: {}\n", doc.arazzo));
    if let Some(self_uri) = &doc.self_uri {
        out.push_str(&format!("/// @ARAZZO_SELF: {}\n", self_uri));
    }
    out.push_str(&format!("/// @ARAZZO_TITLE: {}\n", doc.info.title));
    out.push_str(&format!("/// @ARAZZO_INFO_VERSION: {}\n", doc.info.version));
    if let Some(summary) = &doc.info.summary {
        out.push_str(&format!("/// @ARAZZO_SUMMARY: {}\n", summary));
    }
    if let Some(desc) = &doc.info.description {
        let single_line: String = desc.lines().collect::<Vec<_>>().join(" ");
        out.push_str(&format!("/// @ARAZZO_DESCRIPTION: {}\n", single_line));
    }
    for src in &doc.source_descriptions {
        let st = src.source_type.map(|t| t.as_str()).unwrap_or("openapi");
        out.push_str(&format!(
            "/// @ARAZZO_SOURCE: {}|{}|{}\n",
            src.name, src.url, st
        ));
    }
    out.push_str(&format!("/// @ARAZZO_SPEC_JSON: {}\n", canonical_json));
    out.push_str("/// Client SDK orchestrator for Arazzo 1.1.0 workflows.\n");
    out.push_str("#[derive(Debug, Clone)]\n");
    out.push_str("pub struct WorkflowsClient {\n");
    out.push_str("    /// Underlying HTTP client.\n");
    out.push_str("    pub client: reqwest::Client,\n");
    out.push_str("    /// Canonical `$self` URI of the Arazzo document.\n");
    out.push_str("    pub self_uri: String,\n");
    for src in &doc.source_descriptions {
        let field = format!("{}_base_url", to_snake_case(&src.name));
        out.push_str(&format!(
            "    /// Base URL for source description `{}` (`{}`).\n    pub {}: String,\n",
            src.name, src.url, field
        ));
    }
    out.push_str("}\n\n");

    out.push_str("impl WorkflowsClient {\n");
    out.push_str(
        "    /// Creates a new [`WorkflowsClient`] with default source description URLs.\n",
    );
    out.push_str("    pub fn new(client: reqwest::Client) -> Self {\n");
    out.push_str("        Self {\n");
    out.push_str("            client,\n");
    out.push_str(&format!(
        "            self_uri: {:?}.to_string(),\n",
        doc.self_uri.as_deref().unwrap_or("")
    ));
    for src in &doc.source_descriptions {
        let field = format!("{}_base_url", to_snake_case(&src.name));
        out.push_str(&format!(
            "            {}: {:?}.to_string(),\n",
            field, src.url
        ));
    }
    out.push_str("        }\n");
    out.push_str("    }\n\n");

    for wf in &doc.workflows {
        emit_workflow_method(&mut out, wf, linked_specs);
    }

    out.push_str("}\n");
    Ok(out)
}

/// Emits mock tests for all workflows in `doc` for the Reqwest Client SDK.
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
            "/// @ARAZZO_TEST_WORKFLOW_ID: {}
",
            wf.workflow_id
        ));
        out.push_str(
            "#[tokio::test]
",
        );
        out.push_str(&format!(
            "async fn test_workflow_{}() {{
",
            wf_snake
        ));
        out.push_str(
            "    let http_client = reqwest::Client::new();
",
        );
        out.push_str(&format!(
            "    let wf_client = {}::workflows::WorkflowsClient::new(http_client);
",
            crate_name
        ));
        out.push_str(&format!(
            "    let inputs = {}::workflows::{}Inputs::default();
",
            crate_name, wf_pascal
        ));
        out.push_str(&format!(
            "    let _ = wf_client.{}(inputs).await;
",
            wf_snake
        ));
        out.push_str(
            "}

",
        );
    }
    Ok(out)
}

fn emit_workflow_method(
    out: &mut String,
    wf: &Workflow,
    linked_specs: &BTreeMap<String, ParsedOpenApi>,
) {
    let wf_snake = to_snake_case(&wf.workflow_id);
    let wf_pascal = to_pascal_case(&wf.workflow_id);
    let wf_json =
        serde_json::to_string(&serde_json::to_value(wf).unwrap_or_default()).unwrap_or_default();

    out.push_str(&format!(
        "    /// Executes workflow `{}`.\n",
        wf.workflow_id
    ));
    if let Some(summary) = &wf.summary {
        out.push_str(&format!("    /// {}\n", summary));
    }
    out.push_str(&format!(
        "    /// @ARAZZO_WORKFLOW_ID: {}\n",
        wf.workflow_id
    ));
    if !wf.depends_on.is_empty() {
        out.push_str(&format!(
            "    /// @ARAZZO_DEPENDS_ON: {}\n",
            wf.depends_on.join(",")
        ));
    }
    out.push_str(&format!("    /// @ARAZZO_WORKFLOW_JSON: {}\n", wf_json));
    out.push_str(&format!(
        "    #[allow(unused_variables, unused_mut, clippy::let_and_return)]\n    pub async fn {}(&self, inputs: {}Inputs) -> Result<{}Outputs, WorkflowError> {{\n",
        wf_snake, wf_pascal, wf_pascal
    ));
    out.push_str("        let self_uri = self.self_uri.clone();\n");

    for dep in &wf.depends_on {
        let dep_snake = to_snake_case(dep);
        out.push_str(&format!(
            "        let wf_{}_outputs = self.{}(Default::default()).await?;\n",
            dep_snake, dep_snake
        ));
    }

    let ordered_step_ids = topological_sort_steps(wf).unwrap_or_default();
    for step_id in ordered_step_ids {
        for step in &wf.steps {
            if step.step_id == step_id {
                emit_step_execution(out, wf, step, linked_specs);
            }
        }
    }

    out.push_str(&format!(
        "        let mut outputs = {}Outputs::default();\n",
        wf_pascal
    ));
    for (out_key, out_expr) in &wf.outputs {
        let field_name = to_snake_case(out_key);
        let val_code = match out_expr {
            RuntimeExpressionOrSelector::Expression(expr) => {
                let rust_expr = runtime_expression_to_rust(expr).unwrap_or_default();
                format!("serde_json::to_value({}).unwrap_or_default()", rust_expr)
            }
            RuntimeExpressionOrSelector::Selector(sel) => {
                let ctx_rust = runtime_expression_to_rust(&sel.context).unwrap_or_default();
                format!(
                    "serde_json::to_value({}).unwrap_or_default().pointer({:?}).cloned().unwrap_or_default()",
                    ctx_rust, sel.selector
                )
            }
        };
        out.push_str(&format!("        outputs.{} = {};\n", field_name, val_code));
    }
    out.push_str("        Ok(outputs)\n");
    out.push_str("    }\n\n");
}

fn emit_step_execution(
    out: &mut String,
    wf: &Workflow,
    step: &WorkflowStep,
    linked_specs: &BTreeMap<String, ParsedOpenApi>,
) {
    let wf_pascal = to_pascal_case(&wf.workflow_id);
    let step_snake = to_snake_case(&step.step_id);
    let step_pascal = to_pascal_case(&step.step_id);

    out.push_str(&format!("        // @ARAZZO_STEP_ID: {}\n", step.step_id));
    if !step.depends_on.is_empty() {
        out.push_str(&format!(
            "        // @ARAZZO_STEP_DEPENDS_ON: {}\n",
            step.depends_on.join(",")
        ));
    }

    let first_src_var = "self_uri";
    let resolved_route = resolve_step_operation(step, linked_specs);
    let (http_method, url_expr) = match &step.target {
        StepTarget::OperationId(op_id) => {
            if let Some(route) = &resolved_route {
                (
                    route.method.to_ascii_uppercase(),
                    format!(r#"format!("{{}}{}", {})"#, route.path, first_src_var),
                )
            } else {
                (
                    "POST".to_string(),
                    format!(r#"format!("{{}}/{}", {})"#, op_id, first_src_var),
                )
            }
        }
        StepTarget::OperationPath(op_path) => {
            if let Some(route) = &resolved_route {
                (
                    route.method.to_ascii_uppercase(),
                    format!(r#"format!("{{}}{}", {})"#, route.path, first_src_var),
                )
            } else {
                ("GET".to_string(), format!("{:?}.to_string()", op_path))
            }
        }
        StepTarget::ChannelPath(ch) => (
            "POST".to_string(),
            format!(r#"format!("{{}}{}", {})"#, ch, first_src_var),
        ),
        StepTarget::WorkflowId(sub_wf) => {
            let sub_snake = to_snake_case(sub_wf);
            out.push_str(&format!(
                "        let _sub_wf_{} = self.{}(Default::default()).await?;\n",
                step_snake, sub_snake
            ));
            out.push_str(&format!(
                "        let step_{}_outputs = {}{}Outputs::default();\n",
                step_snake, wf_pascal, step_pascal
            ));
            return;
        }
    };

    out.push_str(&format!("        let mut req_url = {};\n", url_expr));
    out.push_str(&format!(
        "        let req_method = {:?}.to_string();\n",
        http_method
    ));
    out.push_str(
        "        let mut req_headers = std::collections::BTreeMap::<String, String>::new();\n",
    );
    out.push_str(
        "        let mut req_query = std::collections::BTreeMap::<String, String>::new();\n",
    );
    out.push_str(
        "        let mut req_path = std::collections::BTreeMap::<String, String>::new();\n",
    );

    for param in &step.parameters {
        if let ParameterOrReusable::Parameter(p) = param {
            let val_expr = match &p.value {
                ParameterValue::Expression(expr) => {
                    runtime_expression_to_rust(expr).unwrap_or_default()
                }
                ParameterValue::Literal(lit) => format!("{:?}.to_string()", lit.to_string()),
            };
            match p.in_loc {
                Some(crate::arazzo::models::ArazzoParamLocation::Header) => {
                    out.push_str(&format!(
                        r#"        req_headers.insert({:?}.to_string(), format!("{{:?}}", {}));"#,
                        p.name, val_expr
                    ));
                }
                Some(crate::arazzo::models::ArazzoParamLocation::Path) => {
                    let placeholder = format!("{{{}}}", p.name);
                    out.push_str(&format!(
                        r#"        {{ let val_s = format!("{{:?}}", {}); req_path.insert({:?}.to_string(), val_s.clone()); req_url = req_url.replace({:?}, &val_s); }}"#,
                        val_expr, p.name, placeholder
                    ));
                }
                Some(crate::arazzo::models::ArazzoParamLocation::Cookie) => {
                    out.push_str(&format!(
                        r#"        req_headers.insert("Cookie".to_string(), format!("{}={{:?}}", {}));"#,
                        p.name, val_expr
                    ));
                }
                Some(crate::arazzo::models::ArazzoParamLocation::Query)
                | Some(crate::arazzo::models::ArazzoParamLocation::QueryString)
                | None => {
                    out.push_str(&format!(
                        r#"        req_query.insert({:?}.to_string(), format!("{{:?}}", {}));"#,
                        p.name, val_expr
                    ));
                }
            }
            out.push('\n');
        }
    }

    if let Some(rb) = &step.request_body {
        let payload_str =
            serde_json::to_string(&rb.payload.clone().unwrap_or(serde_json::Value::Null))
                .unwrap_or_default();
        out.push_str(&format!(
            "        let mut req_body: serde_json::Value = serde_json::from_str({:?}).unwrap_or_default();\n",
            payload_str
        ));
        for rep in &rb.replacements {
            let rep_val = match &rep.value {
                ParameterValue::Expression(expr) => {
                    let r = runtime_expression_to_rust(expr).unwrap_or_default();
                    format!("serde_json::to_value({}).unwrap_or_default()", r)
                }
                ParameterValue::Literal(lit) => {
                    let s = serde_json::to_string(lit).unwrap_or_default();
                    format!("serde_json::from_str({:?}).unwrap_or_default()", s)
                }
            };
            out.push_str(&format!(
                "        if let Some(slot) = req_body.pointer_mut({:?}) {{ *slot = {}; }}\n",
                rep.target, rep_val
            ));
        }
    } else {
        out.push_str("        let req_body = serde_json::Value::Null;\n");
    }

    let retry_limit = step
        .on_failure
        .iter()
        .find_map(|a| match a {
            ActionOrReusable::Action(FailureAction {
                action_type: FailureActionType::Retry,
                retry_limit: Some(limit),
                ..
            }) => Some(*limit),
            _ => None,
        })
        .unwrap_or(0);
    let retry_after_secs = step.on_failure.iter().find_map(|a| match a {
        ActionOrReusable::Action(FailureAction {
            action_type: FailureActionType::Retry,
            retry_after: Some(secs),
            ..
        }) => Some(*secs),
        _ => None,
    });

    out.push_str(&format!(
        "        let max_attempts: u32 = {};
",
        retry_limit + 1
    ));
    out.push_str(
        "        let mut attempt: u32 = 0;
",
    );
    out.push_str(
        "        let (status_code, resp_body, resp_headers) = loop {
",
    );
    out.push_str(
        "            attempt += 1;
",
    );
    out.push_str(
        "            let method = reqwest::Method::from_bytes(req_method.as_bytes()).map_err(|e| WorkflowError::Execution(e.to_string()))?;
",
    );
    out.push_str(
        "            let mut req_builder = self.client.request(method, &req_url).json(&req_body);
",
    );
    out.push_str(
        "            if !req_query.is_empty() { req_builder = req_builder.query(&req_query); }
",
    );
    out.push_str(
        "            for (k, v) in &req_headers { req_builder = req_builder.header(k, v); }
",
    );
    if let Some(timeout_ms) = step.timeout {
        out.push_str(&format!(
            "            req_builder = req_builder.timeout(std::time::Duration::from_millis({}));
",
            timeout_ms
        ));
    }
    out.push_str(
        "            match req_builder.send().await {
",
    );
    out.push_str(
        "                Ok(resp) => {
",
    );
    out.push_str(
        "                    let code = resp.status().as_u16();
",
    );
    out.push_str(
        "                    let mut hdrs = std::collections::BTreeMap::<String, String>::new();
",
    );
    out.push_str(
        "                    for (k, v) in resp.headers() {
",
    );
    out.push_str("                        if let Ok(s) = v.to_str() { hdrs.insert(k.as_str().to_string(), s.to_string()); }
");
    out.push_str(
        "                    }
",
    );
    out.push_str("                    let body_val: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
");
    out.push_str(
        "                    break (code, body_val, hdrs);
",
    );
    out.push_str(
        "                }
",
    );
    out.push_str(
        "                Err(err) => {
",
    );
    out.push_str(
        "                    if attempt >= max_attempts { return Err(WorkflowError::Http(err)); }
",
    );
    if let Some(secs) = retry_after_secs {
        out.push_str(&format!(
            "                    tokio::time::sleep(std::time::Duration::from_secs_f64({:?})).await;\n",
            secs
        ));
    }
    out.push_str(
        "                }
",
    );
    out.push_str(
        "            }
",
    );
    out.push_str(
        "        };
",
    );
    out.push_str(
        "        let resp_query = req_query.clone();
",
    );
    out.push_str(
        "        let resp_path = req_path.clone();
",
    );
    out.push_str(
        "        let msg_headers = resp_headers.clone();
",
    );
    out.push_str(
        "        let msg_header_json = resp_body.clone();
",
    );
    out.push_str(
        "        let msg_payload = resp_body.clone();
",
    );

    for crit in &step.success_criteria {
        emit_criterion_check(out, crit);
    }

    for action in &step.on_success {
        if let ActionOrReusable::Action(SuccessAction {
            action_type: SuccessActionType::End,
            ..
        }) = action
        {
            out.push_str("        // @ARAZZO_ON_SUCCESS: end\n");
        } else if let ActionOrReusable::Action(SuccessAction {
            action_type: SuccessActionType::Goto,
            target: ActionTarget::StepId(target_step),
            ..
        }) = action
        {
            out.push_str(&format!(
                "        // @ARAZZO_ON_SUCCESS: goto step {}\n",
                target_step
            ));
        }
    }

    out.push_str(&format!(
        "        let mut step_{}_outputs = {}{}Outputs::default();\n",
        step_snake, wf_pascal, step_pascal
    ));
    for (out_key, out_expr) in &step.outputs {
        let field_name = to_snake_case(out_key);
        let val_code = match out_expr {
            RuntimeExpressionOrSelector::Expression(expr) => {
                let rust_expr = runtime_expression_to_rust(expr).unwrap_or_default();
                format!("serde_json::to_value({}).unwrap_or_default()", rust_expr)
            }
            RuntimeExpressionOrSelector::Selector(sel) => {
                let ctx_rust = runtime_expression_to_rust(&sel.context).unwrap_or_default();
                format!(
                    "serde_json::to_value({}).unwrap_or_default().pointer({:?}).cloned().unwrap_or_default()",
                    ctx_rust, sel.selector
                )
            }
        };
        out.push_str(&format!(
            "        step_{}_outputs.{} = {};\n",
            step_snake, field_name, val_code
        ));
    }
}

fn emit_criterion_check(out: &mut String, crit: &Criterion) {
    let ctx_str = crit
        .context
        .as_ref()
        .map(|c| c.as_str())
        .unwrap_or("$statusCode");
    out.push_str(&format!(
        "        // @ARAZZO_CRITERION: context={} condition={}\n",
        ctx_str, crit.condition
    ));
    if matches!(crit.criterion_type, Some(CriterionType::Regex)) {
        out.push_str(&format!(
            "        let re = regex::Regex::new({:?})?;\n        if !re.is_match(&resp_body.to_string()) {{ return Err(WorkflowError::Execution(format!(\"Regex criterion failed: {{}}\", {:?}))); }}\n",
            crit.condition, crit.condition
        ));
    } else if crit.condition == "$statusCode == 200" {
        out.push_str(
            r#"        if status_code != 200 { return Err(WorkflowError::Execution(format!("Expected status 200, got {}", status_code))); }"#,
        );
        out.push('\n');
    }
}

fn emit_parsed_schema_struct(
    out: &mut String,
    fallback_name: &str,
    s: &ParsedStruct,
    comp_name: Option<&str>,
) {
    if let Some(c) = comp_name {
        out.push_str(&format!("/// @ARAZZO_COMPONENT_INPUT: {}\n", c));
    }
    let desc = s.description.as_deref().unwrap_or("Workflow input struct.");
    out.push_str(&format!("/// {}\n", desc));
    out.push_str("#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]\n");
    out.push_str(&format!("pub struct {} {{\n", fallback_name));
    for f in &s.fields {
        let f_desc = f.description.as_deref().unwrap_or("Input field.");
        out.push_str(&format!("    /// {}\n", f_desc));
        if let Some(ren) = &f.rename {
            out.push_str(&format!("    #[serde(rename = {:?})]\n", ren));
        }
        let rust_ty = map_model_field_type(&f.ty);
        out.push_str(&format!("    pub {}: {},\n", f.name, rust_ty));
    }
    out.push_str("}\n\n");
}

fn map_model_field_type(ty: &str) -> String {
    ty.replace("Uuid", "String")
        .replace("DateTime<Utc>", "String")
        .replace("NaiveDate", "String")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arazzo::parse::parse_arazzo_document;
    use ra_ap_edition::Edition;
    use ra_ap_syntax::SourceFile;

    #[test]
    fn test_emit_reqwest_workflow_module_and_tests_valid_ast() {
        let yaml = r##"
arazzo: 1.1.0
$self: https://example.com/arazzo.yaml
info:
  title: Petstore Workflows
  summary: Summary
  description: Desc
  version: 1.0.0
sourceDescriptions:
  - name: petstore
    url: https://petstore.swagger.io/v2
    type: openapi
components:
  inputs:
    OrderInput:
      type: object
      required: [petId]
      properties:
        petId:
          type: string
  parameters:
    sharedParam:
      name: shared
      in: query
      value: 1
workflows:
  - workflowId: loginFlow
    inputs:
      type: object
      properties:
        userName:
          type: string
        user_id:
          type: string
    steps:
      - stepId: authStep
        operationId: loginUser
        parameters:
          - reference: $components.parameters.sharedParam
        successCriteria:
          - condition: $statusCode == 201
        onSuccess:
          - name: gotoWf
            type: goto
            workflowId: buyPet
        outputs:
          token: $response.body#/token
      - stepId: resolvedOpId
        operationId: createOrder
      - stepId: unresolvedOpPath
        operationPath: "{$sourceDescriptions.petstore.url}#/paths/~1missing/get"
  - workflowId: buyPet
    summary: Buy a pet
    dependsOn: [loginFlow]
    inputs:
      $ref: "$components.inputs.OrderInput"
    outputs:
      orderId: $steps.createOrder.outputs.id
      selOut:
        context: $response.body
        selector: /id
    steps:
      - stepId: createOrder
        operationPath: "{$sourceDescriptions.petstore.url}#/paths/~1orders/post"
        timeout: 3000
        parameters:
          - name: petId
            in: query
            value: $inputs.petId
          - name: limit
            in: query
            value: 10
          - name: X-Trace
            in: header
            value: trace-1
          - name: orderId
            in: path
            value: 99
          - name: session
            in: cookie
            value: abc
          - name: defaultLoc
            value: def
        requestBody:
          contentType: application/json
          payload:
            petId: ""
            qty: 1
          replacements:
            - target: /petId
              value: $inputs.petId
            - target: /qty
              value: 2
        successCriteria:
          - condition: $statusCode == 200
          - context: $response.body
            condition: "^ok$"
            type: regex
        onSuccess:
          - name: finish
            type: end
          - name: nextStep
            type: goto
            stepId: notify
        onFailure:
          - name: failEnd
            type: end
          - name: retryOrder
            type: retry
            retryAfter: 1.5
            retryLimit: 2
        outputs:
          id: $response.body#/id
          selStep:
            context: $response.body
            selector: /status
      - stepId: notify
        dependsOn: [createOrder]
        channelPath: /events/order
        action: send
      - stepId: nested
        dependsOn: [notify]
        workflowId: loginFlow
"##;

        let doc = parse_arazzo_document(yaml).expect("parse");
        let mut linked = BTreeMap::new();
        let oas = crate::openapi::parse::parse_openapi_document(
            r#"
openapi: 3.2.0
info:
  title: Petstore
  version: 1.0.0
paths:
  /orders:
    post:
      operationId: createOrder
      responses:
        "200":
          description: ok
"#,
        )
        .expect("oas");
        linked.insert("petstore".to_string(), oas);

        let code = emit_workflow_module(&doc, &linked).expect("emit module");
        let parsed_ast = SourceFile::parse(&code, Edition::Edition2021);
        assert!(
            parsed_ast.errors().is_empty(),
            "AST errors in generated SDK workflow module: {:?}",
            parsed_ast.errors()
        );

        let tests_code = emit_workflow_tests(&doc, "my_sdk").expect("emit tests");
        let parsed_tests = SourceFile::parse(&tests_code, Edition::Edition2021);
        assert!(
            parsed_tests.errors().is_empty(),
            "AST errors in generated SDK workflow tests: {:?}",
            parsed_tests.errors()
        );

        let mut bad_doc = doc;
        bad_doc.workflows.clear();
        assert!(emit_workflow_module(&bad_doc, &linked).is_err());
    }
}
