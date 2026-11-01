#![deny(missing_docs)]

//! # Rust AST -> Arazzo 1.1.0 Workflow Extractor
//!
//! Statically parses Rust source code using `ra_ap_syntax` (Client SDK `WorkflowsClient`,
//! Clap CLI workflow subcommands, Actix/Axum E2E tests, or handwritten async workflow
//! orchestrators) into [`ArazzoDocument`] and [`Workflow`] IR structures with zero code execution.

use crate::arazzo::models::{
    ActionOrReusable, ActionTarget, ArazzoDocument, ArazzoInfo, ArazzoParamLocation,
    ArazzoParameter, ArazzoVersion, Criterion, FailureAction, FailureActionType,
    ParameterOrReusable, ParameterValue, ParsedSchema, RuntimeExpressionOrSelector,
    SourceDescription, SourceDescriptionType, StepTarget, SuccessAction, SuccessActionType,
    Workflow, WorkflowStep,
};
use crate::arazzo::parse::document::to_pascal_case;
use crate::classes::parse::extract_struct;
use crate::classes::parse::models::ParsedStruct;
use crate::error::{AppError, AppResult};
use crate::openapi::parse::models::RuntimeExpression;
use indexmap::IndexMap;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, SourceFile};
use serde_json::{Map, Value as JsonValue};
use std::collections::BTreeMap;

/// Extracts a complete [`ArazzoDocument`] from Rust source code (`WorkflowsClient`, CLI, Server E2E tests, or handwritten async functions).
///
/// # Arguments
/// * `code` - Rust source code string.
/// * `default_title` - Optional fallback title if `@ARAZZO_TITLE` is not present.
/// * `default_version` - Optional fallback version if `@ARAZZO_INFO_VERSION` is not present.
/// * `default_source_url` - Optional fallback OpenAPI source URL if `@ARAZZO_SOURCE` is not present.
///
/// # Returns
/// A validated [`ArazzoDocument`].
///
/// # Errors
/// Returns [`AppError`] if the Rust code has syntax errors or no workflows can be extracted.
pub fn extract_arazzo_document_from_rust(
    code: &str,
    default_title: Option<&str>,
    default_version: Option<&str>,
    default_source_url: Option<&str>,
) -> AppResult<ArazzoDocument> {
    let parsed_file = SourceFile::parse(code, Edition::Edition2021);
    if !parsed_file.errors().is_empty() {
        return Err(AppError::AstParse {
            target: "ArazzoDocument".to_string(),
            details: format!(
                "Rust syntax error while extracting Arazzo workflows: {:?}",
                parsed_file.errors()
            ),
        });
    }

    let mut arazzo_version = ArazzoVersion::V1_1_0;
    let mut self_uri: Option<String> = None;
    let mut title = default_title
        .unwrap_or("Extracted Rust Workflows")
        .to_string();
    let mut info_version = default_version.unwrap_or("0.1.0").to_string();
    let mut summary: Option<String> = None;
    let mut description: Option<String> = None;
    let mut source_descriptions = Vec::new();

    for line in code.lines() {
        let trimmed = line
            .trim()
            .trim_start_matches("///")
            .trim_start_matches("//!")
            .trim_start_matches("//")
            .trim();
        if let Some(v) = trimmed.strip_prefix("@ARAZZO_VERSION:") {
            arazzo_version = ArazzoVersion::parse(v.trim())?;
        } else if let Some(s) = trimmed.strip_prefix("@ARAZZO_SELF:") {
            self_uri = Some(s.trim().to_string());
        } else if let Some(t) = trimmed.strip_prefix("@ARAZZO_TITLE:") {
            if default_title.is_none() {
                title = t.trim().to_string();
            }
        } else if let Some(iv) = trimmed.strip_prefix("@ARAZZO_INFO_VERSION:") {
            if default_version.is_none() {
                info_version = iv.trim().to_string();
            }
        } else if let Some(sum) = trimmed.strip_prefix("@ARAZZO_SUMMARY:") {
            summary = Some(sum.trim().to_string());
        } else if let Some(desc) = trimmed.strip_prefix("@ARAZZO_DESCRIPTION:") {
            description = Some(desc.trim().to_string());
        } else if let Some(src_raw) = trimmed.strip_prefix("@ARAZZO_SOURCE:") {
            let parts: Vec<&str> = src_raw.trim().split('|').collect();
            if parts.len() >= 2 {
                let st = parts
                    .get(2)
                    .and_then(|s| s.parse::<SourceDescriptionType>().ok())
                    .or(Some(SourceDescriptionType::OpenApi));
                source_descriptions.push(SourceDescription {
                    name: parts[0].trim().to_string(),
                    url: parts[1].trim().to_string(),
                    source_type: st,
                    extensions: BTreeMap::new(),
                });
            }
        }
    }

    if source_descriptions.is_empty() {
        source_descriptions.push(SourceDescription {
            name: "default_api".to_string(),
            url: default_source_url.unwrap_or("./openapi.yaml").to_string(),
            source_type: Some(SourceDescriptionType::OpenApi),
            extensions: BTreeMap::new(),
        });
    }

    let workflows = extract_workflows_from_rust(code)?;
    if workflows.is_empty() {
        return Err(AppError::ArazzoValidation {
            path: "/workflows".to_string(),
            reason: "No workflows found in Rust source file".to_string(),
        });
    }

    let doc = ArazzoDocument {
        arazzo: arazzo_version,
        self_uri,
        info: ArazzoInfo {
            title,
            summary,
            description,
            version: info_version,
            extensions: BTreeMap::new(),
        },
        source_descriptions,
        workflows,
        components: None,
        extensions: BTreeMap::new(),
    };

    crate::arazzo::parse::validate_arazzo_document(&doc)?;
    Ok(doc)
}

/// Extracts [`Workflow`] definitions from Rust structs, `impl` methods, and async functions.
///
/// # Arguments
/// * `code` - Rust source code string.
///
/// # Returns
/// A list of extracted [`Workflow`] objects.
///
/// # Errors
/// Returns [`AppError`] if the Rust code fails AST parsing.
pub fn extract_workflows_from_rust(code: &str) -> AppResult<Vec<Workflow>> {
    let parsed_file = SourceFile::parse(code, Edition::Edition2021);
    if !parsed_file.errors().is_empty() {
        return Err(AppError::AstParse {
            target: "Workflows".to_string(),
            details: format!("Rust syntax error: {:?}", parsed_file.errors()),
        });
    }
    let tree = parsed_file.tree();

    let mut struct_schemas: BTreeMap<String, ParsedSchema> = BTreeMap::new();
    for node in tree.syntax().descendants() {
        if let Some(st) = ast::Struct::cast(node) {
            let s_name = st.name().map(|n| n.text().to_string()).unwrap_or_default();
            let mut parsed_st = extract_struct(code, &s_name)?;
            if parsed_st.description.as_deref() == Some("Workflow input struct.") {
                parsed_st.description = None;
            }
            for f in &mut parsed_st.fields {
                if f.description.as_deref() == Some("Input field.") {
                    f.description = None;
                }
            }
            let raw_schema = struct_to_json_schema(&parsed_st);
            struct_schemas.insert(s_name, ParsedSchema::new(raw_schema, parsed_st));
        }
    }

    let mut workflows = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for node in tree.syntax().descendants() {
        if let Some(func) = ast::Fn::cast(node) {
            if let Some(wf) = extract_workflow_from_fn(&func, &struct_schemas)? {
                if seen_ids.insert(wf.workflow_id.clone()) {
                    workflows.push(wf);
                }
            }
        }
    }

    Ok(workflows)
}

/// Executes `extract_workflow_from_fn` (extract workflow from fn).
///
/// # Arguments
/// * `func` - Input `func` parameter.
/// * `struct_schemas` - Input `struct_schemas` parameter.
///
/// # Returns
/// Resulting `AppResult<Option<Workflow>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn extract_workflow_from_fn(
    func: &ast::Fn,
    struct_schemas: &BTreeMap<String, ParsedSchema>,
) -> AppResult<Option<Workflow>> {
    let fn_name = func
        .name()
        .map(|n| n.text().to_string())
        .unwrap_or_default();
    if fn_name == "new" || fn_name == "main" || fn_name == "execute_workflow_cli" {
        return Ok(None);
    }

    let fn_text = func.syntax().to_string();

    let is_async = func.async_token().is_some();
    let has_wf_tag = fn_text.contains("@ARAZZO_WORKFLOW_ID:")
        || fn_name.starts_with("workflow_")
        || fn_name.starts_with("test_workflow_");

    if !is_async && !has_wf_tag {
        return Ok(None);
    }

    let mut workflow_id = if let Some(s) = fn_name.strip_prefix("test_workflow_") {
        s.to_string()
    } else if let Some(s) = fn_name.strip_prefix("handle_workflow_") {
        s.to_string()
    } else if let Some(s) = fn_name.strip_prefix("workflow_") {
        s.to_string()
    } else {
        fn_name.clone()
    };

    let mut summary: Option<String> = None;
    let mut depends_on = Vec::new();
    let mut outputs = IndexMap::new();
    for line in fn_text.lines() {
        if line.contains("fn ") {
            break;
        }
        let trimmed = line.trim().trim_start_matches("///").trim();
        if let Some(id) = trimmed.strip_prefix("@ARAZZO_WORKFLOW_ID:") {
            workflow_id = id.trim().to_string();
        } else if let Some(deps) = trimmed.strip_prefix("@ARAZZO_DEPENDS_ON:") {
            depends_on = split_csv_non_empty(deps);
        } else if let Some((k, re)) = trimmed
            .strip_prefix("@ARAZZO_WORKFLOW_OUTPUT:")
            .and_then(|s| s.split_once('='))
            .and_then(|(k, v)| RuntimeExpression::parse(v.trim()).ok().map(|re| (k, re)))
        {
            outputs.insert(
                k.trim().to_string(),
                RuntimeExpressionOrSelector::Expression(re),
            );
        } else if !trimmed.starts_with('@')
            && !trimmed.starts_with('#')
            && !trimmed.is_empty()
            && !trimmed.starts_with("Executes workflow `")
            && summary.is_none()
        {
            summary = Some(trimmed.to_string());
        }
    }

    let input_struct_name = format!("{}Inputs", to_pascal_case(&workflow_id));
    let inputs = struct_schemas
        .get(&input_struct_name)
        .filter(|s| !s.input_struct.fields.is_empty())
        .cloned();

    let steps = extract_steps_from_fn_body(&fn_text, &workflow_id)?;
    if steps.is_empty() {
        return Ok(None);
    }

    Ok(Some(Workflow {
        workflow_id,
        summary,
        description: None,
        inputs,
        depends_on,
        steps,
        success_actions: Vec::new(),
        failure_actions: Vec::new(),
        outputs,
        parameters: Vec::new(),
        extensions: BTreeMap::new(),
    }))
}

/// Executes `extract_steps_from_fn_body` (extract steps from fn body).
///
/// # Arguments
/// * `fn_text` - Input `fn_text` parameter.
/// * `workflow_id` - Input `workflow_id` parameter.
///
/// # Returns
/// Resulting `AppResult<Vec<WorkflowStep>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn extract_steps_from_fn_body(fn_text: &str, workflow_id: &str) -> AppResult<Vec<WorkflowStep>> {
    let mut steps = Vec::new();
    let mut current_step_id: Option<String> = None;
    let mut current_deps: Vec<String> = Vec::new();
    let mut current_criteria: Vec<Criterion> = Vec::new();
    let mut current_on_success: Vec<ActionOrReusable<SuccessAction>> = Vec::new();
    let mut current_on_failure: Vec<ActionOrReusable<FailureAction>> = Vec::new();
    let mut current_timeout: Option<u64> = None;
    let mut current_params: Vec<ParameterOrReusable> = Vec::new();
    let mut current_outputs: IndexMap<String, RuntimeExpressionOrSelector> = IndexMap::new();
    let mut current_target: Option<StepTarget> = None;

    let flush_step = |steps: &mut Vec<WorkflowStep>,
                      step_id: Option<String>,
                      target: Option<StepTarget>,
                      deps: Vec<String>,
                      params: Vec<ParameterOrReusable>,
                      criteria: Vec<Criterion>,
                      on_success: Vec<ActionOrReusable<SuccessAction>>,
                      on_failure: Vec<ActionOrReusable<FailureAction>>,
                      outputs: IndexMap<String, RuntimeExpressionOrSelector>,
                      timeout: Option<u64>| {
        if let Some(id) = step_id {
            steps.push(WorkflowStep {
                step_id: id.clone(),
                description: None,
                target: target.unwrap_or(StepTarget::OperationId(id)),
                parameters: params,
                request_body: None,
                success_criteria: criteria,
                on_success,
                on_failure,
                outputs,
                timeout,
                correlation_id: None,
                action: None,
                depends_on: deps,
                extensions: BTreeMap::new(),
            });
        }
    };

    for line in fn_text.lines() {
        let trimmed = line.trim();
        if let Some(sid) = trimmed.strip_prefix("// @ARAZZO_STEP_ID:") {
            flush_step(
                &mut steps,
                current_step_id.take(),
                current_target.take(),
                std::mem::take(&mut current_deps),
                std::mem::take(&mut current_params),
                std::mem::take(&mut current_criteria),
                std::mem::take(&mut current_on_success),
                std::mem::take(&mut current_on_failure),
                std::mem::take(&mut current_outputs),
                current_timeout.take(),
            );
            current_step_id = Some(sid.trim().to_string());
        } else if let Some(target_raw) = trimmed.strip_prefix("// @ARAZZO_STEP_TARGET:") {
            let t = target_raw.trim();
            if let Some(op) = t.strip_prefix("operationId=") {
                current_target = Some(StepTarget::OperationId(op.trim().to_string()));
            } else if let Some(p) = t.strip_prefix("operationPath=") {
                current_target = Some(StepTarget::OperationPath(p.trim().to_string()));
            } else if let Some(c) = t.strip_prefix("channelPath=") {
                current_target = Some(StepTarget::ChannelPath(c.trim().to_string()));
            } else if let Some(w) = t.strip_prefix("workflowId=") {
                current_target = Some(StepTarget::WorkflowId(w.trim().to_string()));
            }
        } else if let Some(param_raw) = trimmed.strip_prefix("// @ARAZZO_STEP_PARAM:") {
            let parts: Vec<&str> = param_raw.trim().splitn(3, '|').collect();
            if parts.len() == 3 {
                let loc = parts[1].trim().parse::<ArazzoParamLocation>().ok();
                let val_str = parts[2].trim();
                let val = if val_str.starts_with('$') {
                    ParameterValue::Expression(RuntimeExpression::parse(val_str)?)
                } else {
                    ParameterValue::Literal(
                        serde_json::from_str(val_str)
                            .unwrap_or_else(|_| JsonValue::String(val_str.to_string())),
                    )
                };
                current_params.push(ParameterOrReusable::Parameter(ArazzoParameter {
                    name: parts[0].trim().to_string(),
                    in_loc: loc,
                    value: val,
                    extensions: BTreeMap::new(),
                }));
            }
        } else if let Some((k, re)) = trimmed
            .strip_prefix("// @ARAZZO_STEP_OUTPUT:")
            .and_then(|s| s.split_once('='))
            .and_then(|(k, v)| RuntimeExpression::parse(v.trim()).ok().map(|re| (k, re)))
        {
            current_outputs.insert(
                k.trim().to_string(),
                RuntimeExpressionOrSelector::Expression(re),
            );
        } else if let Some(deps) = trimmed.strip_prefix("// @ARAZZO_STEP_DEPENDS_ON:") {
            current_deps = split_csv_non_empty(deps);
        } else if let Some(crit_raw) = trimmed.strip_prefix("// @ARAZZO_CRITERION:") {
            let mut ctx_opt = None;
            let mut cond = "$statusCode == 200".to_string();
            if let Some((ctx_part, cond_part)) = crit_raw.trim().split_once("condition=") {
                cond = cond_part.trim().to_string();
                if let Some(c) = ctx_part.trim().strip_prefix("context=") {
                    ctx_opt = RuntimeExpression::parse(c.trim()).ok();
                }
            }
            current_criteria.push(Criterion {
                context: ctx_opt,
                condition: cond,
                criterion_type: None,
                extensions: BTreeMap::new(),
            });
        } else if let Some(succ_raw) = trimmed.strip_prefix("// @ARAZZO_ON_SUCCESS:") {
            let s = succ_raw.trim();
            if s == "end" {
                current_on_success.push(ActionOrReusable::Action(SuccessAction {
                    name: "end".to_string(),
                    action_type: SuccessActionType::End,
                    target: ActionTarget::None,
                    parameters: vec![],
                    criteria: vec![],
                    extensions: BTreeMap::new(),
                }));
            } else if let Some(step_target) = s.strip_prefix("goto step ") {
                current_on_success.push(ActionOrReusable::Action(SuccessAction {
                    name: "gotoStep".to_string(),
                    action_type: SuccessActionType::Goto,
                    target: ActionTarget::StepId(step_target.trim().to_string()),
                    parameters: vec![],
                    criteria: vec![],
                    extensions: BTreeMap::new(),
                }));
            }
        } else if let Some(ms_str) = trimmed
            .strip_prefix("req_builder = req_builder.timeout(std::time::Duration::from_millis(")
            .and_then(|s| s.strip_suffix("));"))
        {
            current_timeout = ms_str.trim().parse::<u64>().ok();
        } else if let Some(attempts_str) = trimmed
            .strip_prefix("let max_attempts: u32 = ")
            .and_then(|s| s.strip_suffix(';'))
        {
            if let Ok(max_a) = attempts_str.trim().parse::<u32>() {
                if max_a > 1 {
                    current_on_failure.push(ActionOrReusable::Action(FailureAction {
                        name: "retryStep".to_string(),
                        action_type: FailureActionType::Retry,
                        target: ActionTarget::None,
                        parameters: vec![],
                        retry_after: None,
                        retry_limit: Some(max_a - 1),
                        criteria: vec![],
                        extensions: BTreeMap::new(),
                    }));
                }
            }
        } else if let Some((lhs, rhs)) = trimmed
            .strip_prefix("let step_")
            .filter(|_| trimmed.contains(".await") && current_step_id.is_none())
            .and_then(|_| trimmed.split_once('='))
        {
            // Handwritten Rust workflow step pattern: `let step_<name> = client.<op>(...).await?;`
            let step_var = lhs
                .trim()
                .trim_start_matches("let ")
                .trim_start_matches("mut ")
                .trim();
            let step_id = step_var
                .strip_prefix("step_")
                .unwrap_or(step_var)
                .to_string();
            let op_id = extract_callee_name(rhs).unwrap_or(step_id.clone());
            let params = extract_inline_call_params(rhs);
            steps.push(WorkflowStep {
                step_id,
                description: None,
                target: StepTarget::OperationId(op_id),
                parameters: params,
                request_body: None,
                success_criteria: Vec::new(),
                on_success: Vec::new(),
                on_failure: Vec::new(),
                outputs: IndexMap::new(),
                timeout: None,
                correlation_id: None,
                action: None,
                depends_on: Vec::new(),
                extensions: BTreeMap::new(),
            });
        }
    }

    flush_step(
        &mut steps,
        current_step_id.take(),
        current_target.take(),
        current_deps,
        current_params,
        current_criteria,
        current_on_success,
        current_on_failure,
        current_outputs,
        current_timeout,
    );

    if steps.is_empty() && fn_text.contains(".await") {
        steps.push(WorkflowStep {
            step_id: format!("{}_step_1", workflow_id),
            description: None,
            target: StepTarget::OperationId(workflow_id.to_string()),
            parameters: Vec::new(),
            request_body: None,
            success_criteria: Vec::new(),
            on_success: Vec::new(),
            on_failure: Vec::new(),
            outputs: IndexMap::new(),
            timeout: None,
            correlation_id: None,
            action: None,
            depends_on: Vec::new(),
            extensions: BTreeMap::new(),
        });
    }

    Ok(steps)
}

/// Executes `split_csv_non_empty` (split csv non empty).
///
/// # Arguments
/// * `raw` - Input `raw` parameter.
///
/// # Returns
/// Resulting `Vec<String>` value.
fn split_csv_non_empty(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in raw.split(',') {
        let t = part.trim();
        if !t.is_empty() {
            out.push(t.to_string());
        }
    }
    out
}

/// Executes `extract_callee_name` (extract callee name).
///
/// # Arguments
/// * `rhs` - Input `rhs` parameter.
///
/// # Returns
/// Resulting `Option<String>` value.
fn extract_callee_name(rhs: &str) -> Option<String> {
    let before_paren = rhs.split('(').next().unwrap_or("").trim();
    let last_seg = before_paren.rsplit(['.', ':']).next().unwrap_or("").trim();
    if last_seg.is_empty() {
        None
    } else {
        Some(last_seg.to_string())
    }
}

/// Executes `extract_inline_call_params` (extract inline call params).
///
/// # Arguments
/// * `rhs` - Input `rhs` parameter.
///
/// # Returns
/// Resulting `Vec<ParameterOrReusable>` value.
fn extract_inline_call_params(rhs: &str) -> Vec<ParameterOrReusable> {
    let mut params = Vec::new();
    if let Some((_, after_open)) = rhs.split_once('(') {
        let args_str = after_open
            .rsplit_once(')')
            .map(|(s, _)| s)
            .unwrap_or(after_open);
        for (idx, arg) in args_str.split(',').enumerate() {
            let a = arg.trim();
            if a.is_empty() || a == "&client" || a == "&self.client" || a == "base_url" {
                continue;
            }
            let inp_opt = match a.strip_prefix("inputs.") {
                Some(s) => Some(s),
                None => a.strip_prefix("&inputs."),
            };
            if let Some(inp_field) = inp_opt {
                let clean_field = inp_field.trim_end_matches(".clone()");
                if let Ok(re) = RuntimeExpression::parse(&format!("$inputs.{}", clean_field)) {
                    params.push(ParameterOrReusable::Parameter(ArazzoParameter {
                        name: clean_field.to_string(),
                        in_loc: Some(ArazzoParamLocation::Query),
                        value: ParameterValue::Expression(re),
                        extensions: BTreeMap::new(),
                    }));
                    continue;
                }
            }
            params.push(ParameterOrReusable::Parameter(ArazzoParameter {
                name: format!("arg_{}", idx),
                in_loc: Some(ArazzoParamLocation::Query),
                value: ParameterValue::Literal(JsonValue::String(a.trim_matches('"').to_string())),
                extensions: BTreeMap::new(),
            }));
        }
    }
    params
}

/// Executes `struct_to_json_schema` (struct to json schema).
///
/// # Arguments
/// * `s` - Input `s` parameter.
///
/// # Returns
/// Resulting `JsonValue` value.
fn struct_to_json_schema(s: &ParsedStruct) -> JsonValue {
    let mut props = Map::new();
    let mut req = Vec::new();
    for f in &s.fields {
        let prop_name = f.rename.clone().unwrap_or(f.name.clone());
        let is_opt = f.ty.starts_with("Option<");
        if !is_opt {
            req.push(JsonValue::String(prop_name.clone()));
        }
        let inner_ty =
            f.ty.strip_prefix("Option<")
                .and_then(|t| t.strip_suffix('>'))
                .unwrap_or(&f.ty);
        let json_ty = match inner_ty {
            "i32" | "i64" | "u32" | "u64" | "usize" => "integer",
            "f32" | "f64" => "number",
            "bool" => "boolean",
            _ => "string",
        };
        let mut prop_obj = Map::new();
        prop_obj.insert("type".to_string(), JsonValue::String(json_ty.to_string()));
        if let Some(desc) = f.description.as_deref().filter(|d| *d != "Input field.") {
            prop_obj.insert(
                "description".to_string(),
                JsonValue::String(desc.to_string()),
            );
        }
        props.insert(prop_name, JsonValue::Object(prop_obj));
    }
    let mut schema = Map::new();
    schema.insert("type".to_string(), JsonValue::String("object".to_string()));
    if !req.is_empty() {
        schema.insert("required".to_string(), JsonValue::Array(req));
    }
    schema.insert("properties".to_string(), JsonValue::Object(props));
    JsonValue::Object(schema)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arazzo::parse::parse_arazzo_document;
    use crate::strategies::{
        ActixStrategy, AxumStrategy, ClapCliStrategy, McpClientStrategy, McpServerStrategy,
        ReqwestStrategy, WorkflowStrategy,
    };

    #[test]
    fn test_roundtrip_arazzo_to_rust_and_back_across_all_strategies() {
        let yaml = r##"
arazzo: 1.1.0
$self: https://example.com/workflows/petstore.arazzo.yaml
info:
  title: Petstore Order Flow
  summary: Order flow summary
  description: Order flow description
  version: 1.2.0
sourceDescriptions:
  - name: petstore
    url: ./petstore.openapi.yaml
    type: openapi
workflows:
  - workflowId: placePetOrder
    summary: Find and order a pet
    inputs:
      type: object
      required: [petId]
      properties:
        petId:
          type: string
    steps:
      - stepId: getPetStep
        operationId: getPetById
        parameters:
          - name: petId
            in: path
            value: $inputs.petId
        successCriteria:
          - condition: $statusCode == 200
        outputs:
          foundId: $response.body#/id
    outputs:
      resultId: $steps.getPetStep.outputs.foundId
"##;

        let original_doc = parse_arazzo_document(yaml).expect("parse original");
        let linked = BTreeMap::new();

        for models_code in [
            ReqwestStrategy
                .emit_workflow_models(&original_doc, &linked)
                .expect("reqwest models"),
            ClapCliStrategy
                .emit_workflow_models(&original_doc, &linked)
                .expect("clap models"),
            ActixStrategy
                .emit_workflow_models(&original_doc, &linked)
                .expect("actix models"),
            AxumStrategy
                .emit_workflow_models(&original_doc, &linked)
                .expect("axum models"),
            McpClientStrategy
                .emit_workflow_models(&original_doc, &linked)
                .expect("mcp client models"),
            McpServerStrategy
                .emit_workflow_models(&original_doc, &linked)
                .expect("mcp server models"),
            ReqwestStrategy
                .emit_workflow_tests(&original_doc, "app")
                .expect("reqwest tests"),
            ClapCliStrategy
                .emit_workflow_tests(&original_doc, "app")
                .expect("clap tests"),
            McpClientStrategy
                .emit_workflow_tests(&original_doc, "app")
                .expect("mcp client tests"),
            McpServerStrategy
                .emit_workflow_tests(&original_doc, "app")
                .expect("mcp server tests"),
        ] {
            assert!(!models_code.is_empty());
        }

        for strategy_code in [
            ReqwestStrategy
                .emit_workflow_module(&original_doc, &linked)
                .expect("reqwest"),
            ClapCliStrategy
                .emit_workflow_module(&original_doc, &linked)
                .expect("clap_cli"),
            ActixStrategy
                .emit_workflow_module(&original_doc, &linked)
                .expect("actix mod"),
            ActixStrategy
                .emit_workflow_tests(&original_doc, "app")
                .expect("actix tests"),
            AxumStrategy
                .emit_workflow_module(&original_doc, &linked)
                .expect("axum mod"),
            AxumStrategy
                .emit_workflow_tests(&original_doc, "app")
                .expect("axum tests"),
            McpClientStrategy
                .emit_workflow_module(&original_doc, &linked)
                .expect("mcp client mod"),
            McpServerStrategy
                .emit_workflow_module(&original_doc, &linked)
                .expect("mcp server mod"),
        ] {
            let extracted = extract_arazzo_document_from_rust(&strategy_code, None, None, None)
                .expect("extract");
            assert_eq!(extracted, original_doc);
        }

        let reqwest_mod = ReqwestStrategy
            .emit_workflow_module(&original_doc, &linked)
            .expect("reqwest");
        let wfs_direct = extract_workflows_from_rust(&reqwest_mod).expect("direct wfs");
        assert_eq!(wfs_direct.len(), 1);

        let overridden = extract_arazzo_document_from_rust(
            &reqwest_mod,
            Some("Custom Title"),
            Some("9.9.9"),
            None,
        )
        .expect("overridden");
        assert_eq!(overridden.info.title, "Custom Title");
        assert_eq!(overridden.info.version, "9.9.9");
    }

    #[test]
    fn test_extract_handwritten_rust_workflow_ast() {
        let handwritten_rust = r#"
/// @ARAZZO_VERSION: 1.1.0
/// @ARAZZO_SELF: https://example.com/handwritten.arazzo.yaml
/// @ARAZZO_TITLE: Handwritten Workflow
/// @ARAZZO_INFO_VERSION: 0.2.0
/// @ARAZZO_SUMMARY: Handwritten summary
/// @ARAZZO_DESCRIPTION: Handwritten description
/// @ARAZZO_SOURCE: users_api|./users.yaml|openapi
/// @ARAZZO_SOURCE: invalid_single_part

use serde::{Deserialize, Serialize};

pub struct IgnoredTupleStruct(pub String);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OptionalOnlyInputs {
    pub opt_field: Option<String>,
}

/// Input struct for checkout.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CheckoutFlowInputs {
    /// User identifier.
    pub user_id: String,
    /// Optional coupon.
    pub coupon: Option<String>,
    /// Item count.
    pub count: i32,
    /// Price.
    pub price: f64,
    /// Express shipping.
    pub express: bool,
}

///
/// # Heading ignored
/// Checkout workflow orchestrator.
/// Second summary line ignored.
/// @ARAZZO_WORKFLOW_ID: checkoutFlow
/// @ARAZZO_DEPENDS_ON: fallback, , sync_tagged
/// @ARAZZO_WORKFLOW_OUTPUT: orderId=$steps.chargeCard.outputs.txId
/// @ARAZZO_WORKFLOW_OUTPUT: invalidOutput=bad_expr
pub async fn workflow_checkout_flow(inputs: CheckoutFlowInputs) -> Result<(), String> {
    let step_fetch_cart = client::get_cart(&self.client, base_url, &client, inputs.user_id.clone(), &inputs..clone(), "active").await?;
    let step_no_parens = client_ping.await?;
    let step_anon = ().await;
    let step_not_awaited = 42;
    // @ARAZZO_STEP_ID: chargeCard
    // @ARAZZO_STEP_TARGET: operationId=chargeCardOp
    // @ARAZZO_STEP_TARGET: operationPath=/cards/charge
    // @ARAZZO_STEP_TARGET: channelPath=/events/card
    // @ARAZZO_STEP_TARGET: workflowId=subCharge
    // @ARAZZO_STEP_TARGET: unknownTarget=ignored
    // @ARAZZO_STEP_PARAM: amount|query|100
    // @ARAZZO_STEP_PARAM: label|header|raw_string_val
    // @ARAZZO_STEP_PARAM: userId|path|$inputs.user_id
    // @ARAZZO_STEP_PARAM: malformed_param
    // @ARAZZO_STEP_OUTPUT: txId=$response.body#/txId
    // @ARAZZO_STEP_OUTPUT: badOut=invalid_expr
    // @ARAZZO_STEP_DEPENDS_ON: fetch_cart
    req_builder = req_builder.timeout(std::time::Duration::from_millis(2500));
    let max_attempts: u32 = 3;
    let max_attempts: u32 = 1;
    let max_attempts: u32 = not_a_num;
    // @ARAZZO_CRITERION: context=$statusCode condition=$statusCode == 200
    // @ARAZZO_CRITERION: invalid_no_condition
    // @ARAZZO_CRITERION: no_ctx condition=$statusCode == 200
    // @ARAZZO_CRITERION: context=bad_ctx condition=$statusCode == 200
    // @ARAZZO_ON_SUCCESS: goto step confirmOrder
    // @ARAZZO_ON_SUCCESS: end
    // @ARAZZO_ON_SUCCESS: ignore_other
    // @ARAZZO_STEP_ID: confirmOrder
    Ok(())
}

/// @ARAZZO_WORKFLOW_ID: sync_tagged
/// @ARAZZO_CUSTOM_ANNOTATION: ignored
/// Executes workflow `sync_tagged`.
pub fn tagged_sync_workflow() {
    // @ARAZZO_STEP_ID: syncStep
}

pub fn main() {}
pub fn execute_workflow_cli() {}

pub async fn handle_workflow_server_flow() {
    do_server().await;
}

pub async fn test_workflow_e2e_flow() {
    do_e2e().await;
}

pub async fn empty_async_helper() {}

pub fn sync_helper_in_module() {}

pub mod nested {
    /// Fallback async workflow.
    pub async fn workflow_fallback() {
        do_something().await;
    }
}

pub mod nested_dup {
    /// Duplicate fallback async workflow.
    pub async fn workflow_fallback() {
        do_something().await;
    }
}
"#;

        let doc = extract_arazzo_document_from_rust(handwritten_rust, None, None, None)
            .expect("extract handwritten");
        assert_eq!(doc.info.title, "Handwritten Workflow");
        assert_eq!(doc.info.version, "0.2.0");
        assert_eq!(doc.source_descriptions[0].name, "users_api");
        assert_eq!(doc.workflows.len(), 5);
        assert_eq!(doc.workflows[0].workflow_id, "checkoutFlow");
        assert!(doc.workflows[0].inputs.is_some());
        assert_eq!(doc.workflows[0].steps.len(), 5);
        assert_eq!(doc.workflows[0].steps[0].step_id, "fetch_cart");
        assert_eq!(doc.workflows[0].steps[3].step_id, "chargeCard");
        assert_eq!(doc.workflows[0].steps[3].timeout, Some(2500));

        let min_doc = extract_arazzo_document_from_rust(
            "pub async fn workflow_min() { call().await; }",
            None,
            None,
            None,
        )
        .expect("min doc");
        assert_eq!(min_doc.source_descriptions[0].name, "default_api");
    }

    #[test]
    fn test_extract_rust_ast_error_cases() {
        assert!(extract_arazzo_document_from_rust("fn broken( {", None, None, None).is_err());
        assert!(extract_workflows_from_rust("fn broken( {").is_err());
        assert!(
            extract_arazzo_document_from_rust("pub fn sync_helper() {}", None, None, None).is_err()
        );
        assert!(extract_arazzo_document_from_rust(
            "/// @ARAZZO_VERSION: 9.9.9\npub async fn workflow_x() { a().await; }",
            None,
            None,
            None
        )
        .is_err());
    }
}
