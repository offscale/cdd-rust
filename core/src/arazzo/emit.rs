#![deny(missing_docs)]

//! # Arazzo 1.1.0 Specification Emitter
//!
//! Deterministically serializes [`ArazzoDocument`] Intermediate Representation (IR)
//! models into canonical Arazzo 1.1.0 JSON (`serde_json::Value` / string) and YAML.

use crate::arazzo::models::{
    ActionOrReusable, ActionTarget, ArazzoComponents, ArazzoDocument, ArazzoInfo, ArazzoParameter,
    ArazzoRequestBody, Criterion, CriterionType, FailureAction, ParameterOrReusable, ParsedSchema,
    PayloadReplacement, ReusableObject, RuntimeExpressionOrSelector, SelectorObject,
    SourceDescription, StepTarget, SuccessAction, Workflow, WorkflowStep,
};
use crate::error::AppResult;
use serde_json::{Map, Value as JsonValue};
use std::collections::BTreeMap;

/// Output serialization format for Arazzo specifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArazzoFormat {
    /// Pretty-printed JSON.
    Json,
    /// Canonical YAML.
    Yaml,
}

/// Converts an [`ArazzoDocument`] into a canonical Arazzo 1.1.0 `serde_json::Value` object.
///
/// # Arguments
/// * `doc` - The [`ArazzoDocument`] IR to emit.
///
/// # Returns
/// A canonical `serde_json::Value` representing the Arazzo 1.1.0 document.
///
/// # Errors
/// Returns [`crate::error::AppError`] if validation fails prior to emission.
pub fn generate_arazzo_document(doc: &ArazzoDocument) -> AppResult<JsonValue> {
    crate::arazzo::parse::validate_arazzo_document(doc)?;

    let mut root = Map::new();
    root.insert(
        "arazzo".to_string(),
        JsonValue::String(doc.arazzo.as_str().to_string()),
    );
    if let Some(self_uri) = &doc.self_uri {
        root.insert("$self".to_string(), JsonValue::String(self_uri.clone()));
    }
    root.insert("info".to_string(), emit_info(&doc.info));
    root.insert(
        "sourceDescriptions".to_string(),
        JsonValue::Array(
            doc.source_descriptions
                .iter()
                .map(emit_source_description)
                .collect(),
        ),
    );
    root.insert(
        "workflows".to_string(),
        JsonValue::Array(doc.workflows.iter().map(emit_workflow).collect()),
    );
    if let Some(comps) = &doc.components {
        if !comps.is_empty() {
            root.insert("components".to_string(), emit_components(comps));
        }
    }
    insert_extensions(&mut root, &doc.extensions);

    Ok(JsonValue::Object(root))
}

/// Serializes an [`ArazzoDocument`] into a JSON or YAML string.
///
/// # Arguments
/// * `doc` - The [`ArazzoDocument`] IR to serialize.
/// * `format` - Target [`ArazzoFormat`] (`Json` or `Yaml`).
///
/// # Returns
/// The formatted specification string.
///
/// # Errors
/// Returns [`crate::error::AppError`] if document validation or serialization fails.
pub fn serialize_arazzo_document(doc: &ArazzoDocument, format: ArazzoFormat) -> AppResult<String> {
    let val = generate_arazzo_document(doc)?;
    match format {
        ArazzoFormat::Json => Ok(serde_json::to_string_pretty(&val)?),
        ArazzoFormat::Yaml => Ok(serde_yaml::to_string(&val)?),
    }
}

/// Executes `emit_info` (emit info).
///
/// # Arguments
/// * `info` - Input `info` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_info(info: &ArazzoInfo) -> JsonValue {
    let mut m = Map::new();
    m.insert("title".to_string(), JsonValue::String(info.title.clone()));
    if let Some(s) = &info.summary {
        m.insert("summary".to_string(), JsonValue::String(s.clone()));
    }
    if let Some(d) = &info.description {
        m.insert("description".to_string(), JsonValue::String(d.clone()));
    }
    m.insert(
        "version".to_string(),
        JsonValue::String(info.version.clone()),
    );
    insert_extensions(&mut m, &info.extensions);
    JsonValue::Object(m)
}

/// Executes `emit_source_description` (emit source description).
///
/// # Arguments
/// * `src` - Input `src` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_source_description(src: &SourceDescription) -> JsonValue {
    let mut m = Map::new();
    m.insert("name".to_string(), JsonValue::String(src.name.clone()));
    m.insert("url".to_string(), JsonValue::String(src.url.clone()));
    if let Some(t) = &src.source_type {
        m.insert(
            "type".to_string(),
            JsonValue::String(t.as_str().to_string()),
        );
    }
    insert_extensions(&mut m, &src.extensions);
    JsonValue::Object(m)
}

/// Executes `emit_schema` (emit schema).
///
/// # Arguments
/// * `schema` - Input `schema` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_schema(schema: &ParsedSchema) -> JsonValue {
    if let Some(r) = &schema.reference {
        let mut m = Map::new();
        m.insert("$ref".to_string(), JsonValue::String(r.clone()));
        return JsonValue::Object(m);
    }
    schema.raw_schema.clone()
}

/// Executes `emit_workflow` (emit workflow).
///
/// # Arguments
/// * `wf` - Input `wf` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_workflow(wf: &Workflow) -> JsonValue {
    let mut m = Map::new();
    m.insert(
        "workflowId".to_string(),
        JsonValue::String(wf.workflow_id.clone()),
    );
    if let Some(s) = &wf.summary {
        m.insert("summary".to_string(), JsonValue::String(s.clone()));
    }
    if let Some(d) = &wf.description {
        m.insert("description".to_string(), JsonValue::String(d.clone()));
    }
    if let Some(inp) = &wf.inputs {
        m.insert("inputs".to_string(), emit_schema(inp));
    }
    if !wf.depends_on.is_empty() {
        m.insert(
            "dependsOn".to_string(),
            JsonValue::Array(
                wf.depends_on
                    .iter()
                    .cloned()
                    .map(JsonValue::String)
                    .collect(),
            ),
        );
    }
    m.insert(
        "steps".to_string(),
        JsonValue::Array(wf.steps.iter().map(emit_step).collect()),
    );
    if !wf.success_actions.is_empty() {
        m.insert(
            "successActions".to_string(),
            JsonValue::Array(
                wf.success_actions
                    .iter()
                    .map(emit_success_action_or_reusable)
                    .collect(),
            ),
        );
    }
    if !wf.failure_actions.is_empty() {
        m.insert(
            "failureActions".to_string(),
            JsonValue::Array(
                wf.failure_actions
                    .iter()
                    .map(emit_failure_action_or_reusable)
                    .collect(),
            ),
        );
    }
    if !wf.outputs.is_empty() {
        m.insert("outputs".to_string(), emit_outputs(&wf.outputs));
    }
    if !wf.parameters.is_empty() {
        m.insert(
            "parameters".to_string(),
            JsonValue::Array(
                wf.parameters
                    .iter()
                    .map(emit_parameter_or_reusable)
                    .collect(),
            ),
        );
    }
    insert_extensions(&mut m, &wf.extensions);
    JsonValue::Object(m)
}

/// Executes `emit_step` (emit step).
///
/// # Arguments
/// * `step` - Input `step` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_step(step: &WorkflowStep) -> JsonValue {
    let mut m = Map::new();
    m.insert(
        "stepId".to_string(),
        JsonValue::String(step.step_id.clone()),
    );
    if let Some(d) = &step.description {
        m.insert("description".to_string(), JsonValue::String(d.clone()));
    }
    match &step.target {
        StepTarget::OperationId(id) => {
            m.insert("operationId".to_string(), JsonValue::String(id.clone()));
        }
        StepTarget::OperationPath(p) => {
            m.insert("operationPath".to_string(), JsonValue::String(p.clone()));
        }
        StepTarget::ChannelPath(c) => {
            m.insert("channelPath".to_string(), JsonValue::String(c.clone()));
        }
        StepTarget::WorkflowId(w) => {
            m.insert("workflowId".to_string(), JsonValue::String(w.clone()));
        }
    }
    if !step.parameters.is_empty() {
        m.insert(
            "parameters".to_string(),
            JsonValue::Array(
                step.parameters
                    .iter()
                    .map(emit_parameter_or_reusable)
                    .collect(),
            ),
        );
    }
    if let Some(rb) = &step.request_body {
        m.insert("requestBody".to_string(), emit_request_body(rb));
    }
    if !step.success_criteria.is_empty() {
        m.insert(
            "successCriteria".to_string(),
            JsonValue::Array(step.success_criteria.iter().map(emit_criterion).collect()),
        );
    }
    if !step.on_success.is_empty() {
        m.insert(
            "onSuccess".to_string(),
            JsonValue::Array(
                step.on_success
                    .iter()
                    .map(emit_success_action_or_reusable)
                    .collect(),
            ),
        );
    }
    if !step.on_failure.is_empty() {
        m.insert(
            "onFailure".to_string(),
            JsonValue::Array(
                step.on_failure
                    .iter()
                    .map(emit_failure_action_or_reusable)
                    .collect(),
            ),
        );
    }
    if !step.outputs.is_empty() {
        m.insert("outputs".to_string(), emit_outputs(&step.outputs));
    }
    if let Some(timeout) = step.timeout {
        m.insert("timeout".to_string(), JsonValue::Number(timeout.into()));
    }
    if let Some(corr) = &step.correlation_id {
        m.insert(
            "correlationId".to_string(),
            JsonValue::String(corr.as_str().to_string()),
        );
    }
    if let Some(act) = &step.action {
        m.insert(
            "action".to_string(),
            JsonValue::String(act.as_str().to_string()),
        );
    }
    if !step.depends_on.is_empty() {
        m.insert(
            "dependsOn".to_string(),
            JsonValue::Array(
                step.depends_on
                    .iter()
                    .cloned()
                    .map(JsonValue::String)
                    .collect(),
            ),
        );
    }
    insert_extensions(&mut m, &step.extensions);
    JsonValue::Object(m)
}

/// Executes `emit_parameter_or_reusable` (emit parameter or reusable).
///
/// # Arguments
/// * `item` - Input `item` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_parameter_or_reusable(item: &ParameterOrReusable) -> JsonValue {
    match item {
        ParameterOrReusable::Parameter(p) => emit_parameter(p),
        ParameterOrReusable::Reusable(r) => emit_reusable(r),
    }
}

/// Executes `emit_parameter` (emit parameter).
///
/// # Arguments
/// * `p` - Input `p` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_parameter(p: &ArazzoParameter) -> JsonValue {
    let mut m = Map::new();
    m.insert("name".to_string(), JsonValue::String(p.name.clone()));
    if let Some(loc) = &p.in_loc {
        m.insert(
            "in".to_string(),
            JsonValue::String(loc.as_str().to_string()),
        );
    }
    m.insert("value".to_string(), p.value.to_json_value());
    insert_extensions(&mut m, &p.extensions);
    JsonValue::Object(m)
}

/// Executes `emit_reusable` (emit reusable).
///
/// # Arguments
/// * `r` - Input `r` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_reusable(r: &ReusableObject) -> JsonValue {
    let mut m = Map::new();
    m.insert(
        "reference".to_string(),
        JsonValue::String(r.reference.as_str().to_string()),
    );
    if let Some(val) = &r.value {
        m.insert("value".to_string(), val.to_json_value());
    }
    insert_extensions(&mut m, &r.extensions);
    JsonValue::Object(m)
}

/// Executes `emit_request_body` (emit request body).
///
/// # Arguments
/// * `rb` - Input `rb` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_request_body(rb: &ArazzoRequestBody) -> JsonValue {
    let mut m = Map::new();
    if let Some(ct) = &rb.content_type {
        m.insert("contentType".to_string(), JsonValue::String(ct.clone()));
    }
    if let Some(payload) = &rb.payload {
        m.insert("payload".to_string(), payload.clone());
    }
    if !rb.replacements.is_empty() {
        m.insert(
            "replacements".to_string(),
            JsonValue::Array(rb.replacements.iter().map(emit_replacement).collect()),
        );
    }
    insert_extensions(&mut m, &rb.extensions);
    JsonValue::Object(m)
}

/// Executes `emit_replacement` (emit replacement).
///
/// # Arguments
/// * `rep` - Input `rep` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_replacement(rep: &PayloadReplacement) -> JsonValue {
    let mut m = Map::new();
    m.insert("target".to_string(), JsonValue::String(rep.target.clone()));
    if let Some(st) = &rep.target_selector_type {
        m.insert("targetSelectorType".to_string(), emit_criterion_type(st));
    }
    m.insert("value".to_string(), rep.value.to_json_value());
    insert_extensions(&mut m, &rep.extensions);
    JsonValue::Object(m)
}

/// Executes `emit_criterion` (emit criterion).
///
/// # Arguments
/// * `c` - Input `c` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_criterion(c: &Criterion) -> JsonValue {
    let mut m = Map::new();
    if let Some(ctx) = &c.context {
        m.insert(
            "context".to_string(),
            JsonValue::String(ctx.as_str().to_string()),
        );
    }
    m.insert(
        "condition".to_string(),
        JsonValue::String(c.condition.clone()),
    );
    if let Some(ct) = &c.criterion_type {
        m.insert("type".to_string(), emit_criterion_type(ct));
    }
    insert_extensions(&mut m, &c.extensions);
    JsonValue::Object(m)
}

/// Executes `emit_criterion_type` (emit criterion type).
///
/// # Arguments
/// * `ct` - Input `ct` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_criterion_type(ct: &CriterionType) -> JsonValue {
    match ct {
        CriterionType::Simple => JsonValue::String("simple".to_string()),
        CriterionType::Regex => JsonValue::String("regex".to_string()),
        CriterionType::JsonPath => JsonValue::String("jsonpath".to_string()),
        CriterionType::XPath => JsonValue::String("xpath".to_string()),
        CriterionType::ExpressionType(et) => {
            let mut m = Map::new();
            m.insert(
                "type".to_string(),
                JsonValue::String(et.dialect.as_str().to_string()),
            );
            m.insert("version".to_string(), JsonValue::String(et.version.clone()));
            insert_extensions(&mut m, &et.extensions);
            JsonValue::Object(m)
        }
    }
}

/// Executes `emit_success_action_or_reusable` (emit success action or reusable).
///
/// # Arguments
/// * `item` - Input `item` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_success_action_or_reusable(item: &ActionOrReusable<SuccessAction>) -> JsonValue {
    match item {
        ActionOrReusable::Action(sa) => emit_success_action(sa),
        ActionOrReusable::Reusable(r) => emit_reusable(r),
    }
}

/// Executes `emit_success_action` (emit success action).
///
/// # Arguments
/// * `sa` - Input `sa` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_success_action(sa: &SuccessAction) -> JsonValue {
    let mut m = Map::new();
    m.insert("name".to_string(), JsonValue::String(sa.name.clone()));
    m.insert(
        "type".to_string(),
        JsonValue::String(sa.action_type.as_str().to_string()),
    );
    emit_action_target(&mut m, &sa.target);
    if !sa.parameters.is_empty() {
        m.insert(
            "parameters".to_string(),
            JsonValue::Array(
                sa.parameters
                    .iter()
                    .map(emit_parameter_or_reusable)
                    .collect(),
            ),
        );
    }
    if !sa.criteria.is_empty() {
        m.insert(
            "criteria".to_string(),
            JsonValue::Array(sa.criteria.iter().map(emit_criterion).collect()),
        );
    }
    insert_extensions(&mut m, &sa.extensions);
    JsonValue::Object(m)
}

/// Executes `emit_failure_action_or_reusable` (emit failure action or reusable).
///
/// # Arguments
/// * `item` - Input `item` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_failure_action_or_reusable(item: &ActionOrReusable<FailureAction>) -> JsonValue {
    match item {
        ActionOrReusable::Action(fa) => emit_failure_action(fa),
        ActionOrReusable::Reusable(r) => emit_reusable(r),
    }
}

/// Executes `emit_failure_action` (emit failure action).
///
/// # Arguments
/// * `fa` - Input `fa` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_failure_action(fa: &FailureAction) -> JsonValue {
    let mut m = Map::new();
    m.insert("name".to_string(), JsonValue::String(fa.name.clone()));
    m.insert(
        "type".to_string(),
        JsonValue::String(fa.action_type.as_str().to_string()),
    );
    emit_action_target(&mut m, &fa.target);
    if !fa.parameters.is_empty() {
        m.insert(
            "parameters".to_string(),
            JsonValue::Array(
                fa.parameters
                    .iter()
                    .map(emit_parameter_or_reusable)
                    .collect(),
            ),
        );
    }
    if let Some(ra) = fa.retry_after {
        m.insert("retryAfter".to_string(), serde_json::json!(ra));
    }
    if let Some(rl) = fa.retry_limit {
        m.insert("retryLimit".to_string(), JsonValue::Number(rl.into()));
    }
    if !fa.criteria.is_empty() {
        m.insert(
            "criteria".to_string(),
            JsonValue::Array(fa.criteria.iter().map(emit_criterion).collect()),
        );
    }
    insert_extensions(&mut m, &fa.extensions);
    JsonValue::Object(m)
}

/// Executes `emit_action_target` (emit action target).
///
/// # Arguments
/// * `m` - Input `m` parameter.
/// * `target` - Input `target` parameter.
fn emit_action_target(m: &mut Map<String, JsonValue>, target: &ActionTarget) {
    match target {
        ActionTarget::None => {}
        ActionTarget::WorkflowId(w) => {
            m.insert("workflowId".to_string(), JsonValue::String(w.clone()));
        }
        ActionTarget::StepId(s) => {
            m.insert("stepId".to_string(), JsonValue::String(s.clone()));
        }
    }
}

/// Executes `emit_outputs` (emit outputs).
///
/// # Arguments
/// * `outputs` - Input `outputs` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_outputs(outputs: &indexmap::IndexMap<String, RuntimeExpressionOrSelector>) -> JsonValue {
    let mut m = Map::new();
    for (k, v) in outputs {
        let val = match v {
            RuntimeExpressionOrSelector::Expression(expr) => {
                JsonValue::String(expr.as_str().to_string())
            }
            RuntimeExpressionOrSelector::Selector(sel) => emit_selector(sel),
        };
        m.insert(k.clone(), val);
    }
    JsonValue::Object(m)
}

/// Executes `emit_selector` (emit selector).
///
/// # Arguments
/// * `sel` - Input `sel` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_selector(sel: &SelectorObject) -> JsonValue {
    let mut m = Map::new();
    m.insert(
        "context".to_string(),
        JsonValue::String(sel.context.as_str().to_string()),
    );
    m.insert(
        "selector".to_string(),
        JsonValue::String(sel.selector.clone()),
    );
    if let Some(st) = &sel.selector_type {
        m.insert("type".to_string(), emit_criterion_type(st));
    }
    insert_extensions(&mut m, &sel.extensions);
    JsonValue::Object(m)
}

/// Executes `emit_components` (emit components).
///
/// # Arguments
/// * `comps` - Input `comps` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn emit_components(comps: &ArazzoComponents) -> JsonValue {
    let mut m = Map::new();
    if !comps.inputs.is_empty() {
        let mut inp_map = Map::new();
        for (k, v) in &comps.inputs {
            inp_map.insert(k.clone(), v.raw_schema.clone());
        }
        m.insert("inputs".to_string(), JsonValue::Object(inp_map));
    }
    if !comps.parameters.is_empty() {
        let mut p_map = Map::new();
        for (k, v) in &comps.parameters {
            p_map.insert(k.clone(), emit_parameter(v));
        }
        m.insert("parameters".to_string(), JsonValue::Object(p_map));
    }
    if !comps.success_actions.is_empty() {
        let mut sa_map = Map::new();
        for (k, v) in &comps.success_actions {
            sa_map.insert(k.clone(), emit_success_action(v));
        }
        m.insert("successActions".to_string(), JsonValue::Object(sa_map));
    }
    if !comps.failure_actions.is_empty() {
        let mut fa_map = Map::new();
        for (k, v) in &comps.failure_actions {
            fa_map.insert(k.clone(), emit_failure_action(v));
        }
        m.insert("failureActions".to_string(), JsonValue::Object(fa_map));
    }
    insert_extensions(&mut m, &comps.extensions);
    JsonValue::Object(m)
}

/// Executes `insert_extensions` (insert extensions).
///
/// # Arguments
/// * `m` - Input `m` parameter.
/// * `extensions` - Input `extensions` parameter.
fn insert_extensions(m: &mut Map<String, JsonValue>, extensions: &BTreeMap<String, JsonValue>) {
    for (k, v) in extensions {
        m.insert(k.clone(), v.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arazzo::parse::parse_arazzo_document;

    #[test]
    fn test_lossless_arazzo_parse_and_emit_roundtrip() {
        let yaml = r##"
arazzo: 1.1.0
$self: https://example.com/workflows/petstore.arazzo.yaml
x-root: true
info:
  title: Petstore Workflow
  summary: Summary text
  description: Description text
  version: 1.0.0
  x-info: 42
sourceDescriptions:
  - name: petstore
    url: ./petstore.yaml
    type: openapi
    x-src: "ok"
components:
  x-comp: 1
  inputs:
    PetInput:
      type: object
      required: [petId]
      properties:
        petId:
          type: string
  parameters:
    sharedToken:
      name: X-Token
      in: header
      value: $inputs.petId
  successActions:
    sharedSuccess:
      name: done
      type: goto
      workflowId: wf1
      parameters:
        - name: p
          in: query
          value: "1"
      criteria:
        - condition: $statusCode == 200
          type: xpath
        - condition: $statusCode == 200
          type: simple
  failureActions:
    sharedFail:
      name: failGoto
      type: goto
      stepId: s1
      parameters:
        - name: p
          in: cookie
          value: "2"
      criteria:
        - condition: $statusCode == 500
          type: jsonpath
        - condition: $statusCode == 503
workflows:
  - workflowId: wf1
    summary: Wf summary
    description: Wf desc
    inputs:
      $ref: "#/components/inputs/PetInput"
    parameters:
      - reference: $components.parameters.sharedToken
        value: "override"
    successActions:
      - reference: $components.successActions.sharedSuccess
    failureActions:
      - reference: $components.failureActions.sharedFail
    outputs:
      out1: $steps.s1.outputs.stepOut
      out2:
        context: $response.body
        selector: $.id
        type:
          type: jsonpath
          version: rfc9535
    steps:
      - stepId: s1
        description: Step 1
        operationId: getPet
        timeout: 1000
        parameters:
          - name: q
            in: querystring
            value: $inputs.petId
        requestBody:
          contentType: application/json
          payload:
            id: "0"
          replacements:
            - target: /id
              targetSelectorType: simple
              value: $inputs.petId
        successCriteria:
          - context: $statusCode
            condition: "^200$"
            type: regex
        onSuccess:
          - name: endStep
            type: end
        onFailure:
          - name: retryStep
            type: retry
            retryAfter: 1.5
            retryLimit: 3
        outputs:
          stepOut: $response.body#/id
"##;

        let doc1 = parse_arazzo_document(yaml).expect("parse 1");
        let emitted_yaml = serialize_arazzo_document(&doc1, ArazzoFormat::Yaml).expect("emit yaml");
        let emitted_json = serialize_arazzo_document(&doc1, ArazzoFormat::Json).expect("emit json");

        let doc_from_yaml = parse_arazzo_document(&emitted_yaml).expect("re-parse yaml");
        let doc_from_json = parse_arazzo_document(&emitted_json).expect("re-parse json");

        assert_eq!(doc1, doc_from_yaml);
        assert_eq!(doc1, doc_from_json);

        // Cover inline workflow inputs, OperationPath, ChannelPath, WorkflowId, step correlationId/action/dependsOn/Reusable, and empty components
        let yaml2 = "arazzo: 1.1.0\ninfo:\n  title: Minimal\n  version: 1.0.0\nsourceDescriptions:\n  - name: s1\n    url: ./s1.yaml\ncomponents: {}\nworkflows:\n  - workflowId: w0\n    steps:\n      - stepId: st0\n        operationId: op0\n  - workflowId: w1\n    dependsOn: [w0]\n    inputs:\n      type: object\n      properties:\n        a:\n          type: string\n    parameters:\n      - name: wfParam\n        in: header\n        value: 'v'\n    successActions:\n      - name: wfEnd\n        type: end\n    failureActions:\n      - name: wfFail\n        type: end\n    steps:\n      - stepId: st1\n        operationPath: '{$sourceDescriptions.s1.url}#/paths/~1p/get'\n        parameters:\n          - name: p1\n            value: 'v1'\n        requestBody:\n          replacements:\n            - target: /a\n              value: 'b'\n        outputs:\n          o1:\n            context: $response.body\n            selector: /a\n      - stepId: st2\n        dependsOn: [st1]\n        channelPath: /ch\n        action: receive\n        correlationId: $response.body#/id\n      - stepId: st3\n        dependsOn: [st2]\n        workflowId: w0\n";
        let mut doc2 = parse_arazzo_document(yaml2).expect("parse 2");
        doc2.components = None;
        doc2.workflows[0].steps[0].success_criteria.push(Criterion {
            context: None,
            condition: "$statusCode == 200".to_string(),
            criterion_type: None,
            extensions: BTreeMap::new(),
        });
        let json2_none = serialize_arazzo_document(&doc2, ArazzoFormat::Json).expect("emit 2 none");
        assert!(parse_arazzo_document(&json2_none).is_ok());
        doc2.components = Some(ArazzoComponents::default());
        let json2_empty = serialize_arazzo_document(&doc2, ArazzoFormat::Json).expect("emit 2a");
        assert!(parse_arazzo_document(&json2_empty).is_ok());
        let mut empty_comps = ArazzoComponents::default();
        empty_comps
            .extensions
            .insert("x-comp".to_string(), JsonValue::Bool(true));
        doc2.components = Some(empty_comps);
        let json2 = serialize_arazzo_document(&doc2, ArazzoFormat::Json).expect("emit 2b");
        assert!(parse_arazzo_document(&json2).is_ok());

        // Also test Reusable in step parameters, step onSuccess, and step onFailure
        let yaml3 = "arazzo: 1.1.0\ninfo:\n  title: Step Reusables\n  version: 1.0.0\nsourceDescriptions:\n  - name: s1\n    url: ./s1.yaml\ncomponents:\n  parameters:\n    p1:\n      name: p\n      in: query\n      value: '1'\n  successActions:\n    sa1:\n      name: s\n      type: end\n  failureActions:\n    fa1:\n      name: f\n      type: end\nworkflows:\n  - workflowId: w1\n    steps:\n      - stepId: st1\n        operationId: op1\n        requestBody: {}\n        parameters:\n          - reference: $components.parameters.p1\n        onSuccess:\n          - reference: $components.successActions.sa1\n        onFailure:\n          - reference: $components.failureActions.fa1\n";
        let doc3 = parse_arazzo_document(yaml3).expect("parse 3");
        let json3 = serialize_arazzo_document(&doc3, ArazzoFormat::Json).expect("emit 3");
        assert_eq!(parse_arazzo_document(&json3).expect("reparse 3"), doc3);
    }
}
