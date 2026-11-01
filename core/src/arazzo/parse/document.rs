#![deny(missing_docs)]

//! # Arazzo 1.1.0 Document Parser, Validator & Cross-Spec Resolver
//!
//! Parses YAML or JSON Arazzo 1.1.0 specifications into [`ArazzoDocument`], validates
//! structural invariants, performs DAG topological sorting and cycle detection on
//! `workflow.dependsOn` and `step.dependsOn`, validates `$components.*` references,
//! and resolves linked OpenAPI specifications via [`DocumentRegistry`].

use crate::arazzo::models::{
    ActionOrReusable, ActionTarget, ArazzoComponents, ArazzoDocument, ArazzoInfo,
    ArazzoParamLocation, ArazzoParameter, ArazzoRequestBody, ArazzoVersion, AsyncActionType,
    Criterion, CriterionExpressionType, CriterionType, ExpressionDialect, FailureAction,
    FailureActionType, ParameterOrReusable, ParameterValue, ParsedSchema, PayloadReplacement,
    ReusableObject, RuntimeExpressionOrSelector, SelectorObject, SourceDescription,
    SourceDescriptionType, StepTarget, SuccessAction, SuccessActionType, Workflow, WorkflowStep,
};
use crate::arazzo::parse::expressions::{ArazzoExpressionRoot, ComponentSection};
use crate::classes::parse::models::{ParsedField, ParsedStruct};
use crate::error::{AppError, AppResult};
use crate::openapi::parse::document::{parse_openapi_document_with_registry, ParsedOpenApi};
use crate::openapi::parse::models::{ParsedRoute, RuntimeExpression, RustIdent};
use crate::openapi::parse::ref_utils::decode_pointer_segment;
use crate::openapi::parse::registry::DocumentRegistry;
use crate::openapi::parse::routes::naming::to_snake_case;
use indexmap::IndexMap;
use serde_json::{Map, Value as JsonValue};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use url::Url;

/// Converts an identifier to PascalCase.
///
/// # Arguments
/// * `s` - Input `s` parameter.
///
/// # Returns
/// Resulting `String` value.
pub fn to_pascal_case(s: &str) -> String {
    RustIdent::sanitize_pascal(s, "WorkflowModel").into_inner()
}

/// Parses and validates an Arazzo 1.1.0 YAML or JSON specification into an [`ArazzoDocument`].
///
/// # Arguments
/// * `content` - Raw YAML or JSON Arazzo specification string.
///
/// # Returns
/// A validated [`ArazzoDocument`].
///
/// # Errors
/// Returns [`AppError`] on YAML/JSON syntax errors, missing required fields, unsupported versions,
/// cyclic dependencies, invalid runtime expressions, or unresolved `$components` references.
pub fn parse_arazzo_document(content: &str) -> AppResult<ArazzoDocument> {
    let mut registry = DocumentRegistry::new();
    parse_arazzo_document_with_registry(content, None, &mut registry)
}

/// Parses and validates an Arazzo 1.1.0 YAML or JSON specification with a [`DocumentRegistry`].
///
/// # Arguments
/// * `content` - Raw YAML or JSON Arazzo specification string.
/// * `base_uri` - Optional base URI for resolving `$self` and relative `sourceDescriptions`.
/// * `_registry` - Mutable [`DocumentRegistry`] for cross-specification resolution.
///
/// # Returns
/// A validated [`ArazzoDocument`].
///
/// # Errors
/// Returns [`AppError`] if parsing or validation fails.
pub fn parse_arazzo_document_with_registry(
    content: &str,
    base_uri: Option<&Url>,
    _registry: &mut DocumentRegistry,
) -> AppResult<ArazzoDocument> {
    let root_val: JsonValue = serde_yaml::from_str(content)?;
    let obj = expect_object(
        &root_val,
        "/",
        "Arazzo root document must be a JSON/YAML mapping",
    )?;

    let arazzo_str = get_required_str(obj, "arazzo", "ArazzoDocument")?;
    let arazzo = ArazzoVersion::parse(arazzo_str)?;

    let self_uri = match obj.get("$self") {
        Some(JsonValue::String(s)) => Some(resolve_self_uri(s, base_uri)?),
        Some(_) => {
            return Err(AppError::ArazzoValidation {
                path: "/$self".to_string(),
                reason: "'$self' must be a string URI".to_string(),
            })
        }
        None => None,
    };

    let info_val = get_required_val(obj, "info", "ArazzoDocument")?;
    let info = parse_arazzo_info(info_val)?;

    let sources_val = get_required_val(obj, "sourceDescriptions", "ArazzoDocument")?;
    let source_descriptions = parse_source_descriptions(sources_val)?;

    let components = match obj.get("components") {
        Some(c_val) => Some(parse_components(c_val)?),
        None => None,
    };

    let workflows_val = get_required_val(obj, "workflows", "ArazzoDocument")?;
    let workflows = parse_workflows(workflows_val, components.as_ref())?;

    let extensions = extract_extensions(obj);

    let doc = ArazzoDocument {
        arazzo,
        self_uri,
        info,
        source_descriptions,
        workflows,
        components,
        extensions,
    };

    validate_arazzo_document(&doc)?;
    Ok(doc)
}

/// Executes `resolve_self_uri` (resolve self uri).
///
/// # Arguments
/// * `s` - Input `s` parameter.
/// * `base_uri` - Input `base_uri` parameter.
///
/// # Returns
/// Resulting `AppResult<String>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn resolve_self_uri(s: &str, base_uri: Option<&Url>) -> AppResult<String> {
    let parsed = match base_uri {
        Some(base) => base.join(s),
        None => Url::parse(s),
    };
    match parsed {
        Ok(u) => Ok(if base_uri.is_some() {
            u.to_string()
        } else {
            s.to_string()
        }),
        Err(e) => Err(AppError::ArazzoValidation {
            path: "/$self".to_string(),
            reason: format!("Invalid $self URI '{}': {}", s, e),
        }),
    }
}

/// Validates structural invariants, component references, and dependency DAGs of an [`ArazzoDocument`].
///
/// # Arguments
/// * `doc` - The parsed [`ArazzoDocument`].
///
/// # Returns
/// `Ok(())` if all invariants hold.
///
/// # Errors
/// Returns [`AppError`] on duplicate identifiers, cyclic dependencies, or missing component references.
pub fn validate_arazzo_document(doc: &ArazzoDocument) -> AppResult<()> {
    if doc.source_descriptions.is_empty() {
        return Err(AppError::ArazzoValidation {
            path: "/sourceDescriptions".to_string(),
            reason: "'sourceDescriptions' must contain at least one entry".to_string(),
        });
    }

    let mut seen_sources = HashSet::new();
    for (idx, src) in doc.source_descriptions.iter().enumerate() {
        let valid_chars = src
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b));
        if src.name.is_empty() || !valid_chars {
            return Err(AppError::ArazzoValidation {
                path: format!("/sourceDescriptions/{}/name", idx),
                reason: format!("Invalid sourceDescription name '{}'", src.name),
            });
        }
        if !seen_sources.insert(src.name.clone()) {
            return Err(AppError::ArazzoValidation {
                path: format!("/sourceDescriptions/{}/name", idx),
                reason: format!("Duplicate sourceDescription name '{}'", src.name),
            });
        }
    }

    if doc.workflows.is_empty() {
        return Err(AppError::ArazzoValidation {
            path: "/workflows".to_string(),
            reason: "'workflows' must contain at least one workflow".to_string(),
        });
    }

    let mut seen_workflows = HashSet::new();
    for (wf_idx, wf) in doc.workflows.iter().enumerate() {
        if wf.workflow_id.trim().is_empty() {
            return Err(AppError::ArazzoValidation {
                path: format!("/workflows/{}/workflowId", wf_idx),
                reason: "'workflowId' must not be empty".to_string(),
            });
        }
        if !seen_workflows.insert(wf.workflow_id.clone()) {
            return Err(AppError::ArazzoValidation {
                path: format!("/workflows/{}/workflowId", wf_idx),
                reason: format!("Duplicate workflowId '{}'", wf.workflow_id),
            });
        }
        if wf.steps.is_empty() {
            return Err(AppError::ArazzoValidation {
                path: format!("/workflows/{}/steps", wf_idx),
                reason: format!(
                    "Workflow '{}' must contain at least one step",
                    wf.workflow_id
                ),
            });
        }

        let mut seen_steps = HashSet::new();
        for (st_idx, step) in wf.steps.iter().enumerate() {
            if step.step_id.trim().is_empty() {
                return Err(AppError::ArazzoValidation {
                    path: format!("/workflows/{}/steps/{}/stepId", wf_idx, st_idx),
                    reason: "'stepId' must not be empty".to_string(),
                });
            }
            if !seen_steps.insert(step.step_id.clone()) {
                return Err(AppError::ArazzoValidation {
                    path: format!("/workflows/{}/steps/{}/stepId", wf_idx, st_idx),
                    reason: format!(
                        "Duplicate stepId '{}' in workflow '{}'",
                        step.step_id, wf.workflow_id
                    ),
                });
            }
            validate_step_components(step, doc.components.as_ref())?;
        }

        for param in &wf.parameters {
            validate_param_or_reusable(param, doc.components.as_ref())?;
        }
        for sa in &wf.success_actions {
            validate_success_action_or_reusable(sa, doc.components.as_ref())?;
        }
        for fa in &wf.failure_actions {
            validate_failure_action_or_reusable(fa, doc.components.as_ref())?;
        }

        let _ = topological_sort_steps(wf)?;
    }

    let _ = topological_sort_workflows(&doc.workflows)?;
    Ok(())
}

/// Performs topological sorting and cycle detection across workflows using `workflow.dependsOn`.
///
/// # Arguments
/// * `workflows` - Slice of [`Workflow`] objects.
///
/// # Returns
/// Ordered `Vec<String>` of `workflowId`s respecting `dependsOn` prerequisites.
///
/// # Errors
/// Returns [`AppError::WorkflowCycleDetected`] on cycles or [`AppError::ArazzoValidation`] on unknown `dependsOn` IDs.
pub fn topological_sort_workflows(workflows: &[Workflow]) -> AppResult<Vec<String>> {
    let mut adj: IndexMap<String, Vec<String>> = IndexMap::new();
    for wf in workflows {
        adj.insert(wf.workflow_id.clone(), wf.depends_on.clone());
    }

    for (wf_id, deps) in &adj {
        for dep in deps {
            if !adj.contains_key(dep) {
                return Err(AppError::ArazzoValidation {
                    path: format!("workflows.{}.dependsOn", wf_id),
                    reason: format!("Unknown prerequisite workflowId '{}'", dep),
                });
            }
        }
    }

    topological_sort_graph(&adj, "workflows")
}

/// Performs topological sorting and cycle detection across steps within a [`Workflow`] using `step.dependsOn`.
///
/// # Arguments
/// * `workflow` - The [`Workflow`] whose steps should be validated.
///
/// # Returns
/// Ordered `Vec<String>` of `stepId`s respecting `step.dependsOn` prerequisites.
///
/// # Errors
/// Returns [`AppError::WorkflowCycleDetected`] on cycles or [`AppError::ArazzoValidation`] on unknown `dependsOn` step IDs.
pub fn topological_sort_steps(workflow: &Workflow) -> AppResult<Vec<String>> {
    let mut adj: IndexMap<String, Vec<String>> = IndexMap::new();
    for step in &workflow.steps {
        adj.insert(step.step_id.clone(), step.depends_on.clone());
    }

    for (step_id, deps) in &adj {
        for dep in deps {
            if !adj.contains_key(dep) {
                return Err(AppError::ArazzoValidation {
                    path: format!(
                        "workflows.{}.steps.{}.dependsOn",
                        workflow.workflow_id, step_id
                    ),
                    reason: format!("Unknown prerequisite stepId '{}'", dep),
                });
            }
        }
    }

    topological_sort_graph(&adj, &workflow.workflow_id)
}

/// Resolves OpenAPI specifications referenced in `doc.source_descriptions` from `base_dir` or `registry`.
///
/// # Arguments
/// * `doc` - The parsed [`ArazzoDocument`].
/// * `base_dir` - Optional directory path for resolving relative `sourceDescriptions.url` file paths.
/// * `registry` - Mutable [`DocumentRegistry`] used for cross-document reference resolution.
///
/// # Returns
/// A map from `sourceDescription.name` to [`ParsedOpenApi`].
///
/// # Errors
/// Returns [`AppError::SourceDescriptionResolution`] if a referenced local OpenAPI file is invalid.
pub fn resolve_source_descriptions(
    doc: &ArazzoDocument,
    base_dir: Option<&Path>,
    registry: &mut DocumentRegistry,
) -> AppResult<BTreeMap<String, ParsedOpenApi>> {
    let mut resolved = BTreeMap::new();
    for src in &doc.source_descriptions {
        let is_openapi = matches!(src.source_type, None | Some(SourceDescriptionType::OpenApi));
        if !is_openapi {
            continue;
        }

        if let Some(dir) = base_dir {
            let candidate = dir.join(&src.url);
            if candidate.is_file() {
                let raw_yaml = std::fs::read_to_string(&candidate)?;
                registry.register_openapi_yaml(&src.url, &raw_yaml)?;
                let parsed = parse_openapi_document_with_registry(
                    &raw_yaml,
                    Some(registry),
                    Some(&src.url),
                )?;
                resolved.insert(src.name.clone(), parsed);
            }
        }
    }
    Ok(resolved)
}

/// Resolves a [`WorkflowStep`]'s `operationId` or `operationPath` against linked [`ParsedOpenApi`] specifications.
///
/// # Arguments
/// * `step` - The [`WorkflowStep`] to resolve.
/// * `linked_specs` - Map of `sourceDescription.name -> ParsedOpenApi`.
///
/// # Returns
/// The matching [`ParsedRoute`] if found in `linked_specs`.
pub fn resolve_step_operation(
    step: &WorkflowStep,
    linked_specs: &BTreeMap<String, ParsedOpenApi>,
) -> Option<ParsedRoute> {
    match &step.target {
        StepTarget::OperationId(op_ref) => {
            let trimmed = op_ref
                .strip_prefix("$sourceDescriptions.")
                .unwrap_or(op_ref.as_str());
            if let Some((src_name, op_id)) = trimmed.split_once('.') {
                if let Some(spec) = linked_specs.get(src_name) {
                    for r in &spec.routes {
                        if r.operation_id.as_deref() == Some(op_id) {
                            return Some(r.clone());
                        }
                    }
                }
            }
            for spec in linked_specs.values() {
                for r in &spec.routes {
                    if r.operation_id.as_deref() == Some(trimmed) {
                        return Some(r.clone());
                    }
                }
            }
            None
        }
        StepTarget::OperationPath(op_path) => {
            let (_, pointer) = op_path.split_once('#')?;
            let segments: Vec<String> = pointer
                .split('/')
                .skip(1)
                .map(decode_pointer_segment)
                .collect();
            if segments.len() >= 3 && segments[0] == "paths" {
                let target_path = &segments[1];
                let target_method = segments[2].to_ascii_uppercase();
                for spec in linked_specs.values() {
                    for r in &spec.routes {
                        if &r.path == target_path && r.method.eq_ignore_ascii_case(&target_method) {
                            return Some(r.clone());
                        }
                    }
                }
            }
            None
        }
        StepTarget::ChannelPath(_) | StepTarget::WorkflowId(_) => None,
    }
}

/// Executes `topological_sort_graph` (topological sort graph).
///
/// # Arguments
/// * `adj` - Input `adj` parameter.
/// * `scope_name` - Input `scope_name` parameter.
///
/// # Returns
/// Resulting `AppResult<Vec<String>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn topological_sort_graph(
    adj: &IndexMap<String, Vec<String>>,
    scope_name: &str,
) -> AppResult<Vec<String>> {
    let mut visited = HashSet::new();
    let mut visiting = Vec::new();
    let mut order = Vec::new();

    for node in adj.keys() {
        if !visited.contains(node) {
            dfs_visit(
                node,
                adj,
                &mut visited,
                &mut visiting,
                &mut order,
                scope_name,
            )?;
        }
    }

    Ok(order)
}

/// Executes `dfs_visit` (dfs visit).
///
/// # Arguments
/// * `node` - Input `node` parameter.
/// * `adj` - Input `adj` parameter.
/// * `visited` - Input `visited` parameter.
/// * `visiting` - Input `visiting` parameter.
/// * `order` - Input `order` parameter.
/// * `scope_name` - Input `scope_name` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn dfs_visit(
    node: &str,
    adj: &IndexMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    visiting: &mut Vec<String>,
    order: &mut Vec<String>,
    scope_name: &str,
) -> AppResult<()> {
    if let Some(pos) = visiting.iter().position(|n| n == node) {
        let mut chain: Vec<String> = visiting[pos..].to_vec();
        chain.push(node.to_string());
        return Err(AppError::WorkflowCycleDetected {
            workflow_id: scope_name.to_string(),
            chain: chain.join(" -> "),
        });
    }
    if visited.contains(node) {
        return Ok(());
    }

    visiting.push(node.to_string());
    for dep in &adj[node] {
        dfs_visit(dep, adj, visited, visiting, order, scope_name)?;
    }
    let _ = visiting.pop();
    visited.insert(node.to_string());
    order.push(node.to_string());
    Ok(())
}

/// Executes `validate_step_components` (validate step components).
///
/// # Arguments
/// * `step` - Input `step` parameter.
/// * `components` - Input `components` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_step_components(
    step: &WorkflowStep,
    components: Option<&ArazzoComponents>,
) -> AppResult<()> {
    for param in &step.parameters {
        validate_param_or_reusable(param, components)?;
    }
    for sa in &step.on_success {
        validate_success_action_or_reusable(sa, components)?;
    }
    for fa in &step.on_failure {
        validate_failure_action_or_reusable(fa, components)?;
    }
    Ok(())
}

/// Executes `validate_param_or_reusable` (validate param or reusable).
///
/// # Arguments
/// * `item` - Input `item` parameter.
/// * `components` - Input `components` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_param_or_reusable(
    item: &ParameterOrReusable,
    components: Option<&ArazzoComponents>,
) -> AppResult<()> {
    if let ParameterOrReusable::Reusable(r) = item {
        let root = ArazzoExpressionRoot::parse(r.reference.as_str())?;
        match root {
            ArazzoExpressionRoot::Components {
                section: ComponentSection::Parameters,
                name,
            } => {
                let exists = components.is_some_and(|c| c.parameters.contains_key(&name));
                if !exists {
                    return Err(AppError::ReferenceResolution {
                        reference: r.reference.to_string(),
                        reason: format!(
                            "Unresolved parameter component '$components.parameters.{}'",
                            name
                        ),
                    });
                }
            }
            _ => {
                return Err(AppError::ArazzoValidation {
                    path: "parameter.reference".to_string(),
                    reason: format!(
                        "Parameter reusable reference must target '$components.parameters.<name>', got '{}'",
                        r.reference
                    ),
                });
            }
        }
    }
    Ok(())
}

/// Executes `validate_success_action_or_reusable` (validate success action or reusable).
///
/// # Arguments
/// * `item` - Input `item` parameter.
/// * `components` - Input `components` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_success_action_or_reusable(
    item: &ActionOrReusable<SuccessAction>,
    components: Option<&ArazzoComponents>,
) -> AppResult<()> {
    match item {
        ActionOrReusable::Action(sa) => {
            for p in &sa.parameters {
                validate_param_or_reusable(p, components)?;
            }
        }
        ActionOrReusable::Reusable(r) => {
            let root = ArazzoExpressionRoot::parse(r.reference.as_str())?;
            match root {
                ArazzoExpressionRoot::Components {
                    section: ComponentSection::SuccessActions,
                    name,
                } => {
                    let exists = components.is_some_and(|c| c.success_actions.contains_key(&name));
                    if !exists {
                        return Err(AppError::ReferenceResolution {
                            reference: r.reference.to_string(),
                            reason: format!(
                                "Unresolved successAction component '$components.successActions.{}'",
                                name
                            ),
                        });
                    }
                }
                _ => {
                    return Err(AppError::ArazzoValidation {
                        path: "successAction.reference".to_string(),
                        reason: format!(
                            "SuccessAction reusable reference must target '$components.successActions.<name>', got '{}'",
                            r.reference
                        ),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Executes `validate_failure_action_or_reusable` (validate failure action or reusable).
///
/// # Arguments
/// * `item` - Input `item` parameter.
/// * `components` - Input `components` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_failure_action_or_reusable(
    item: &ActionOrReusable<FailureAction>,
    components: Option<&ArazzoComponents>,
) -> AppResult<()> {
    match item {
        ActionOrReusable::Action(fa) => {
            for p in &fa.parameters {
                validate_param_or_reusable(p, components)?;
            }
        }
        ActionOrReusable::Reusable(r) => {
            let root = ArazzoExpressionRoot::parse(r.reference.as_str())?;
            match root {
                ArazzoExpressionRoot::Components {
                    section: ComponentSection::FailureActions,
                    name,
                } => {
                    let exists = components.is_some_and(|c| c.failure_actions.contains_key(&name));
                    if !exists {
                        return Err(AppError::ReferenceResolution {
                            reference: r.reference.to_string(),
                            reason: format!(
                                "Unresolved failureAction component '$components.failureActions.{}'",
                                name
                            ),
                        });
                    }
                }
                _ => {
                    return Err(AppError::ArazzoValidation {
                        path: "failureAction.reference".to_string(),
                        reason: format!(
                            "FailureAction reusable reference must target '$components.failureActions.<name>', got '{}'",
                            r.reference
                        ),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Executes `parse_arazzo_info` (parse arazzo info).
///
/// # Arguments
/// * `val` - Input `val` parameter.
///
/// # Returns
/// Resulting `AppResult<ArazzoInfo>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_arazzo_info(val: &JsonValue) -> AppResult<ArazzoInfo> {
    let obj = expect_object(val, "/info", "'info' must be a mapping")?;
    let title = get_required_str(obj, "title", "Info")?.to_string();
    let version = get_required_str(obj, "version", "Info")?.to_string();
    let summary = obj
        .get("summary")
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned);
    let description = obj
        .get("description")
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned);
    let extensions = extract_extensions(obj);

    Ok(ArazzoInfo {
        title,
        summary,
        description,
        version,
        extensions,
    })
}

/// Executes `parse_source_descriptions` (parse source descriptions).
///
/// # Arguments
/// * `val` - Input `val` parameter.
///
/// # Returns
/// Resulting `AppResult<Vec<SourceDescription>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_source_descriptions(val: &JsonValue) -> AppResult<Vec<SourceDescription>> {
    let arr = expect_array(
        val,
        "/sourceDescriptions",
        "'sourceDescriptions' must be an array",
    )?;
    let mut out = Vec::with_capacity(arr.len());
    for (idx, item) in arr.iter().enumerate() {
        let obj = expect_object(
            item,
            &format!("/sourceDescriptions/{}", idx),
            "sourceDescription entry must be an object",
        )?;
        let name = get_required_str(obj, "name", "SourceDescription")?.to_string();
        let url = get_required_str(obj, "url", "SourceDescription")?.to_string();
        let source_type = match obj.get("type").and_then(JsonValue::as_str) {
            Some(t) => Some(t.parse::<SourceDescriptionType>()?),
            None => None,
        };
        let extensions = extract_extensions(obj);
        out.push(SourceDescription {
            name,
            url,
            source_type,
            extensions,
        });
    }
    Ok(out)
}

/// Executes `parse_components` (parse components).
///
/// # Arguments
/// * `val` - Input `val` parameter.
///
/// # Returns
/// Resulting `AppResult<ArazzoComponents>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_components(val: &JsonValue) -> AppResult<ArazzoComponents> {
    let obj = expect_object(val, "/components", "'components' must be an object")?;

    let mut inputs = IndexMap::new();
    if let Some(inp_map) = obj.get("inputs").and_then(JsonValue::as_object) {
        for (name, schema_val) in inp_map {
            inputs.insert(name.clone(), parse_input_schema(name, schema_val, None)?);
        }
    }

    let mut parameters = IndexMap::new();
    if let Some(param_map) = obj.get("parameters").and_then(JsonValue::as_object) {
        for (name, p_val) in param_map {
            parameters.insert(
                name.clone(),
                parse_parameter_object(p_val, &format!("/components/parameters/{}", name))?,
            );
        }
    }

    let mut success_actions = IndexMap::new();
    if let Some(sa_map) = obj.get("successActions").and_then(JsonValue::as_object) {
        for (name, sa_val) in sa_map {
            success_actions.insert(
                name.clone(),
                parse_success_action_object(
                    sa_val,
                    &format!("/components/successActions/{}", name),
                )?,
            );
        }
    }

    let mut failure_actions = IndexMap::new();
    if let Some(fa_map) = obj.get("failureActions").and_then(JsonValue::as_object) {
        for (name, fa_val) in fa_map {
            failure_actions.insert(
                name.clone(),
                parse_failure_action_object(
                    fa_val,
                    &format!("/components/failureActions/{}", name),
                )?,
            );
        }
    }

    let extensions = extract_extensions(obj);
    Ok(ArazzoComponents {
        inputs,
        parameters,
        success_actions,
        failure_actions,
        extensions,
    })
}

/// Executes `parse_workflows` (parse workflows).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `components` - Input `components` parameter.
///
/// # Returns
/// Resulting `AppResult<Vec<Workflow>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_workflows(
    val: &JsonValue,
    components: Option<&ArazzoComponents>,
) -> AppResult<Vec<Workflow>> {
    let arr = expect_array(val, "/workflows", "'workflows' must be an array")?;

    let mut out = Vec::with_capacity(arr.len());
    for (idx, item) in arr.iter().enumerate() {
        let path = format!("/workflows/{}", idx);
        let obj = expect_object(item, &path, "workflow entry must be an object")?;

        let workflow_id = get_required_str(obj, "workflowId", "Workflow")?.to_string();
        let summary = obj
            .get("summary")
            .and_then(JsonValue::as_str)
            .map(ToOwned::to_owned);
        let description = obj
            .get("description")
            .and_then(JsonValue::as_str)
            .map(ToOwned::to_owned);

        let inputs = match obj.get("inputs") {
            Some(inp_val) => Some(parse_input_schema(
                &format!("{}Inputs", to_pascal_case(&workflow_id)),
                inp_val,
                components,
            )?),
            None => None,
        };

        let depends_on = parse_string_list(obj.get("dependsOn"), &format!("{}/dependsOn", path))?;

        let steps_val = get_required_val(obj, "steps", "Workflow")?;
        let steps = parse_steps(steps_val, &format!("{}/steps", path))?;

        let success_actions = match obj.get("successActions") {
            Some(v) => parse_success_actions_list(v, &format!("{}/successActions", path))?,
            None => Vec::new(),
        };
        let failure_actions = match obj.get("failureActions") {
            Some(v) => parse_failure_actions_list(v, &format!("{}/failureActions", path))?,
            None => Vec::new(),
        };
        let outputs = match obj.get("outputs") {
            Some(v) => parse_outputs_map(v, &format!("{}/outputs", path))?,
            None => IndexMap::new(),
        };
        let parameters = match obj.get("parameters") {
            Some(v) => parse_parameters_list(v, &format!("{}/parameters", path))?,
            None => Vec::new(),
        };
        let extensions = extract_extensions(obj);

        out.push(Workflow {
            workflow_id,
            summary,
            description,
            inputs,
            depends_on,
            steps,
            success_actions,
            failure_actions,
            outputs,
            parameters,
            extensions,
        });
    }
    Ok(out)
}

/// Executes `parse_input_schema` (parse input schema).
///
/// # Arguments
/// * `struct_name` - Input `struct_name` parameter.
/// * `schema_val` - Input `schema_val` parameter.
/// * `components` - Input `components` parameter.
///
/// # Returns
/// Resulting `AppResult<ParsedSchema>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_input_schema(
    struct_name: &str,
    schema_val: &JsonValue,
    components: Option<&ArazzoComponents>,
) -> AppResult<ParsedSchema> {
    let ref_opt = match schema_val.get("$ref").and_then(JsonValue::as_str) {
        Some(r) => Some(r),
        None => schema_val.as_str(),
    };
    if let Some(ref_str) = ref_opt {
        let comp_opt = match ref_str.strip_prefix("#/components/inputs/") {
            Some(c) => Some(c),
            None => ref_str.strip_prefix("$components.inputs."),
        };
        if let Some(comp_name) = comp_opt {
            if let Some(comps) = components {
                if let Some(resolved) = comps.inputs.get(comp_name) {
                    let mut cloned = resolved.clone();
                    cloned.reference = Some(ref_str.to_string());
                    return Ok(cloned);
                }
            }
            return Err(AppError::ReferenceResolution {
                reference: ref_str.to_string(),
                reason: format!("Unresolved input schema component '{}'", ref_str),
            });
        }
    }

    let mut fields = Vec::new();
    let mut required_set = HashSet::new();
    if let Some(arr) = schema_val.get("required").and_then(JsonValue::as_array) {
        for item in arr {
            if let Some(s) = item.as_str() {
                required_set.insert(s.to_string());
            }
        }
    }

    if let Some(props) = schema_val.get("properties").and_then(JsonValue::as_object) {
        for (prop_name, prop_schema) in props {
            let base_ty = json_schema_to_rust_type(prop_schema);
            let is_req = required_set.contains(prop_name);
            let ty = if is_req {
                base_ty
            } else {
                format!("Option<{}>", base_ty)
            };
            let field_name = to_snake_case(prop_name);
            let rename = if field_name != *prop_name {
                Some(prop_name.clone())
            } else {
                None
            };
            fields.push(ParsedField {
                name: field_name,
                ty,
                description: prop_schema
                    .get("description")
                    .and_then(JsonValue::as_str)
                    .map(ToOwned::to_owned),
                rename,
                is_skipped: false,
                is_read_only: false,
                is_write_only: false,
                is_deprecated: prop_schema
                    .get("deprecated")
                    .and_then(JsonValue::as_bool)
                    .unwrap_or(false),
                external_docs: None,
            });
        }
    }

    let input_struct = ParsedStruct {
        name: to_pascal_case(struct_name),
        description: schema_val
            .get("description")
            .and_then(JsonValue::as_str)
            .map(ToOwned::to_owned),
        rename: None,
        rename_all: None,
        fields,
        is_deprecated: false,
        deny_unknown_fields: false,
        external_docs: None,
    };

    Ok(ParsedSchema::new(schema_val.clone(), input_struct))
}

/// Executes `json_schema_to_rust_type` (json schema to rust type).
///
/// # Arguments
/// * `schema` - Input `schema` parameter.
///
/// # Returns
/// Resulting `String` value.
fn json_schema_to_rust_type(schema: &JsonValue) -> String {
    match schema.get("type").and_then(JsonValue::as_str) {
        Some("string") => match schema.get("format").and_then(JsonValue::as_str) {
            Some("uuid") => "Uuid".to_string(),
            Some("date-time") => "DateTime<Utc>".to_string(),
            Some("date") => "NaiveDate".to_string(),
            _ => "String".to_string(),
        },
        Some("integer") => match schema.get("format").and_then(JsonValue::as_str) {
            Some("int32") => "i32".to_string(),
            _ => "i64".to_string(),
        },
        Some("number") => match schema.get("format").and_then(JsonValue::as_str) {
            Some("float") => "f32".to_string(),
            _ => "f64".to_string(),
        },
        Some("boolean") => "bool".to_string(),
        Some("array") => {
            let item_ty = schema
                .get("items")
                .map(json_schema_to_rust_type)
                .unwrap_or(String::from("serde_json::Value"));
            format!("Vec<{}>", item_ty)
        }
        _ => "serde_json::Value".to_string(),
    }
}

/// Executes `parse_steps` (parse steps).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<Vec<WorkflowStep>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_steps(val: &JsonValue, path: &str) -> AppResult<Vec<WorkflowStep>> {
    let arr = expect_array(val, path, "'steps' must be an array")?;
    let mut out = Vec::with_capacity(arr.len());
    for (idx, item) in arr.iter().enumerate() {
        let step_path = format!("{}/{}", path, idx);
        let obj = expect_object(item, &step_path, "step entry must be an object")?;
        let step_id = get_required_str(obj, "stepId", "Step")?.to_string();
        let description = obj
            .get("description")
            .and_then(JsonValue::as_str)
            .map(ToOwned::to_owned);
        let target = parse_step_target(obj, &step_path)?;

        let parameters = match obj.get("parameters") {
            Some(v) => parse_parameters_list(v, &format!("{}/parameters", step_path))?,
            None => Vec::new(),
        };
        let request_body = match obj.get("requestBody") {
            Some(v) => Some(parse_request_body(
                v,
                &format!("{}/requestBody", step_path),
            )?),
            None => None,
        };
        let success_criteria = match obj.get("successCriteria") {
            Some(v) => parse_criteria_list(v, &format!("{}/successCriteria", step_path))?,
            None => Vec::new(),
        };
        let on_success = match obj.get("onSuccess") {
            Some(v) => parse_success_actions_list(v, &format!("{}/onSuccess", step_path))?,
            None => Vec::new(),
        };
        let on_failure = match obj.get("onFailure") {
            Some(v) => parse_failure_actions_list(v, &format!("{}/onFailure", step_path))?,
            None => Vec::new(),
        };
        let outputs = match obj.get("outputs") {
            Some(v) => parse_outputs_map(v, &format!("{}/outputs", step_path))?,
            None => IndexMap::new(),
        };
        let timeout = obj.get("timeout").and_then(JsonValue::as_u64);
        let correlation_id = match obj.get("correlationId").and_then(JsonValue::as_str) {
            Some(s) => Some(RuntimeExpression::parse(s)?),
            None => None,
        };
        let action = match obj.get("action").and_then(JsonValue::as_str) {
            Some(s) => Some(s.parse::<AsyncActionType>()?),
            None => None,
        };
        let depends_on =
            parse_string_list(obj.get("dependsOn"), &format!("{}/dependsOn", step_path))?;
        let extensions = extract_extensions(obj);

        out.push(WorkflowStep {
            step_id,
            description,
            target,
            parameters,
            request_body,
            success_criteria,
            on_success,
            on_failure,
            outputs,
            timeout,
            correlation_id,
            action,
            depends_on,
            extensions,
        });
    }
    Ok(out)
}

/// Executes `parse_step_target` (parse step target).
///
/// # Arguments
/// * `obj` - Input `obj` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<StepTarget>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_step_target(obj: &Map<String, JsonValue>, path: &str) -> AppResult<StepTarget> {
    let op_id = obj.get("operationId").and_then(JsonValue::as_str);
    let op_path = obj.get("operationPath").and_then(JsonValue::as_str);
    let ch_path = obj.get("channelPath").and_then(JsonValue::as_str);
    let wf_id = obj.get("workflowId").and_then(JsonValue::as_str);

    let count = [
        op_id.is_some(),
        op_path.is_some(),
        ch_path.is_some(),
        wf_id.is_some(),
    ]
    .into_iter()
    .filter(|b| *b)
    .count();

    if count == 0 {
        return Err(AppError::ArazzoValidation {
            path: path.to_string(),
            reason: "Step must specify exactly one of 'operationId', 'operationPath', 'channelPath', or 'workflowId'".to_string(),
        });
    }
    if count > 1 {
        return Err(AppError::ArazzoValidation {
            path: path.to_string(),
            reason: "Step target fields ('operationId', 'operationPath', 'channelPath', 'workflowId') are mutually exclusive".to_string(),
        });
    }

    if let Some(id) = op_id {
        return Ok(StepTarget::OperationId(id.to_string()));
    }
    if let Some(p) = op_path {
        let _ = RuntimeExpression::parse(p)?;
        return Ok(StepTarget::OperationPath(p.to_string()));
    }
    if let Some(c) = ch_path {
        return Ok(StepTarget::ChannelPath(c.to_string()));
    }
    Ok(StepTarget::WorkflowId(
        wf_id.unwrap_or_default().to_string(),
    ))
}

/// Executes `parse_parameters_list` (parse parameters list).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<Vec<ParameterOrReusable>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_parameters_list(val: &JsonValue, path: &str) -> AppResult<Vec<ParameterOrReusable>> {
    let arr = expect_array(val, path, "'parameters' must be an array")?;
    let mut out = Vec::with_capacity(arr.len());
    for (idx, item) in arr.iter().enumerate() {
        let p_path = format!("{}/{}", path, idx);
        let obj = expect_object(item, &p_path, "parameter entry must be an object")?;
        if obj.contains_key("reference") {
            out.push(ParameterOrReusable::Reusable(parse_reusable_object(
                obj, &p_path,
            )?));
        } else {
            out.push(ParameterOrReusable::Parameter(parse_parameter_object(
                item, &p_path,
            )?));
        }
    }
    Ok(out)
}

/// Executes `parse_parameter_object` (parse parameter object).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<ArazzoParameter>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_parameter_object(val: &JsonValue, path: &str) -> AppResult<ArazzoParameter> {
    let obj = expect_object(val, path, "parameter must be an object")?;
    let name = get_required_str(obj, "name", "Parameter")?.to_string();
    let in_loc = match obj.get("in").and_then(JsonValue::as_str) {
        Some(s) => Some(s.parse::<ArazzoParamLocation>()?),
        None => None,
    };
    let val_raw = get_required_val(obj, "value", "Parameter")?;
    let value = parse_parameter_value(val_raw)?;
    let extensions = extract_extensions(obj);

    Ok(ArazzoParameter {
        name,
        in_loc,
        value,
        extensions,
    })
}

/// Executes `parse_parameter_value` (parse parameter value).
///
/// # Arguments
/// * `val` - Input `val` parameter.
///
/// # Returns
/// Resulting `AppResult<ParameterValue>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_parameter_value(val: &JsonValue) -> AppResult<ParameterValue> {
    if let Some(s) = val.as_str() {
        let expr = RuntimeExpression::parse(s)?;
        if expr.is_expression() {
            return Ok(ParameterValue::Expression(expr));
        }
    }
    Ok(ParameterValue::Literal(val.clone()))
}

/// Executes `parse_reusable_object` (parse reusable object).
///
/// # Arguments
/// * `obj` - Input `obj` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<ReusableObject>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_reusable_object(obj: &Map<String, JsonValue>, _path: &str) -> AppResult<ReusableObject> {
    let ref_str = get_required_str(obj, "reference", "ReusableObject")?;
    let reference = RuntimeExpression::parse_expression(ref_str)?;
    let value = match obj.get("value") {
        Some(v) => Some(parse_parameter_value(v)?),
        None => None,
    };
    let extensions = extract_extensions(obj);
    Ok(ReusableObject {
        reference,
        value,
        extensions,
    })
}

/// Executes `parse_request_body` (parse request body).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<ArazzoRequestBody>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_request_body(val: &JsonValue, path: &str) -> AppResult<ArazzoRequestBody> {
    let obj = expect_object(val, path, "'requestBody' must be an object")?;
    let content_type = obj
        .get("contentType")
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned);
    let payload = obj.get("payload").cloned();
    if let Some(JsonValue::String(s)) = &payload {
        let _ = RuntimeExpression::parse(s)?;
    }

    let mut replacements = Vec::new();
    if let Some(rep_arr) = obj.get("replacements").and_then(JsonValue::as_array) {
        for (idx, rep_val) in rep_arr.iter().enumerate() {
            let r_path = format!("{}/replacements/{}", path, idx);
            let r_obj = expect_object(rep_val, &r_path, "replacement entry must be an object")?;
            let target = get_required_str(r_obj, "target", "PayloadReplacement")?.to_string();
            let sel_val = match r_obj.get("targetSelectorType") {
                Some(t) => Some(t),
                None => r_obj.get("type"),
            };
            let target_selector_type = match sel_val {
                Some(t) => Some(parse_criterion_type(
                    t,
                    &format!("{}/targetSelectorType", r_path),
                )?),
                None => None,
            };
            let val_raw = get_required_val(r_obj, "value", "PayloadReplacement")?;
            let value = parse_parameter_value(val_raw)?;
            let extensions = extract_extensions(r_obj);
            replacements.push(PayloadReplacement {
                target,
                target_selector_type,
                value,
                extensions,
            });
        }
    }

    let extensions = extract_extensions(obj);
    Ok(ArazzoRequestBody {
        content_type,
        payload,
        replacements,
        extensions,
    })
}

/// Executes `parse_criteria_list` (parse criteria list).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<Vec<Criterion>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_criteria_list(val: &JsonValue, path: &str) -> AppResult<Vec<Criterion>> {
    let arr = expect_array(val, path, "'criteria' must be an array")?;
    let mut out = Vec::with_capacity(arr.len());
    for (idx, item) in arr.iter().enumerate() {
        let c_path = format!("{}/{}", path, idx);
        let obj = expect_object(item, &c_path, "criterion must be an object")?;
        let condition = get_required_str(obj, "condition", "Criterion")?.to_string();
        let context = match obj.get("context").and_then(JsonValue::as_str) {
            Some(s) => Some(RuntimeExpression::parse(s)?),
            None => None,
        };
        let criterion_type = match obj.get("type") {
            Some(t) => Some(parse_criterion_type(t, &format!("{}/type", c_path))?),
            None => None,
        };
        let extensions = extract_extensions(obj);
        out.push(Criterion {
            context,
            condition,
            criterion_type,
            extensions,
        });
    }
    Ok(out)
}

/// Executes `parse_criterion_type` (parse criterion type).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<CriterionType>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_criterion_type(val: &JsonValue, path: &str) -> AppResult<CriterionType> {
    if let Some(s) = val.as_str() {
        return CriterionType::from_type_str(s);
    }
    if let Some(obj) = val.as_object() {
        let type_str = get_required_str(obj, "type", "CriterionExpressionType")?;
        let dialect = type_str.parse::<ExpressionDialect>()?;
        let version = get_required_str(obj, "version", "CriterionExpressionType")?.to_string();
        dialect.validate_version(&version)?;
        let extensions = extract_extensions(obj);
        return Ok(CriterionType::ExpressionType(CriterionExpressionType {
            dialect,
            version,
            extensions,
        }));
    }
    Err(AppError::ArazzoValidation {
        path: path.to_string(),
        reason: "Criterion 'type' must be a string or CriterionExpressionType object".to_string(),
    })
}

/// Executes `parse_success_actions_list` (parse success actions list).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<Vec<ActionOrReusable<SuccessAction>>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_success_actions_list(
    val: &JsonValue,
    path: &str,
) -> AppResult<Vec<ActionOrReusable<SuccessAction>>> {
    let arr = expect_array(val, path, "'successActions' must be an array")?;
    let mut out = Vec::with_capacity(arr.len());
    for (idx, item) in arr.iter().enumerate() {
        let a_path = format!("{}/{}", path, idx);
        let obj = expect_object(item, &a_path, "successAction must be an object")?;
        if obj.contains_key("reference") {
            out.push(ActionOrReusable::Reusable(parse_reusable_object(
                obj, &a_path,
            )?));
        } else {
            out.push(ActionOrReusable::Action(parse_success_action_object(
                item, &a_path,
            )?));
        }
    }
    Ok(out)
}

/// Executes `parse_success_action_object` (parse success action object).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<SuccessAction>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_success_action_object(val: &JsonValue, path: &str) -> AppResult<SuccessAction> {
    let obj = expect_object(val, path, "successAction must be an object")?;
    let name = get_required_str(obj, "name", "SuccessAction")?.to_string();
    let type_str = get_required_str(obj, "type", "SuccessAction")?;
    let action_type = type_str.parse::<SuccessActionType>()?;
    let target = parse_action_target(obj, path)?;
    if action_type == SuccessActionType::Goto && target == ActionTarget::None {
        return Err(AppError::ArazzoValidation {
            path: path.to_string(),
            reason: "SuccessAction with type 'goto' must specify 'workflowId' or 'stepId'"
                .to_string(),
        });
    }
    let parameters = match obj.get("parameters") {
        Some(v) => parse_parameters_list(v, &format!("{}/parameters", path))?,
        None => Vec::new(),
    };
    let criteria = match obj.get("criteria") {
        Some(v) => parse_criteria_list(v, &format!("{}/criteria", path))?,
        None => Vec::new(),
    };
    let extensions = extract_extensions(obj);
    Ok(SuccessAction {
        name,
        action_type,
        target,
        parameters,
        criteria,
        extensions,
    })
}

/// Executes `parse_failure_actions_list` (parse failure actions list).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<Vec<ActionOrReusable<FailureAction>>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_failure_actions_list(
    val: &JsonValue,
    path: &str,
) -> AppResult<Vec<ActionOrReusable<FailureAction>>> {
    let arr = expect_array(val, path, "'failureActions' must be an array")?;
    let mut out = Vec::with_capacity(arr.len());
    for (idx, item) in arr.iter().enumerate() {
        let a_path = format!("{}/{}", path, idx);
        let obj = expect_object(item, &a_path, "failureAction must be an object")?;
        if obj.contains_key("reference") {
            out.push(ActionOrReusable::Reusable(parse_reusable_object(
                obj, &a_path,
            )?));
        } else {
            out.push(ActionOrReusable::Action(parse_failure_action_object(
                item, &a_path,
            )?));
        }
    }
    Ok(out)
}

/// Executes `parse_failure_action_object` (parse failure action object).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<FailureAction>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_failure_action_object(val: &JsonValue, path: &str) -> AppResult<FailureAction> {
    let obj = expect_object(val, path, "failureAction must be an object")?;
    let name = get_required_str(obj, "name", "FailureAction")?.to_string();
    let type_str = get_required_str(obj, "type", "FailureAction")?;
    let action_type = type_str.parse::<FailureActionType>()?;
    let target = parse_action_target(obj, path)?;
    if action_type == FailureActionType::Goto && target == ActionTarget::None {
        return Err(AppError::ArazzoValidation {
            path: path.to_string(),
            reason: "FailureAction with type 'goto' must specify 'workflowId' or 'stepId'"
                .to_string(),
        });
    }
    let retry_after = obj.get("retryAfter").and_then(JsonValue::as_f64);
    if let Some(ra) = retry_after {
        if ra < 0.0 {
            return Err(AppError::ArazzoValidation {
                path: format!("{}/retryAfter", path),
                reason: "'retryAfter' must be non-negative".to_string(),
            });
        }
    }
    let retry_limit = obj
        .get("retryLimit")
        .and_then(JsonValue::as_u64)
        .map(|v| v as u32);
    let parameters = match obj.get("parameters") {
        Some(v) => parse_parameters_list(v, &format!("{}/parameters", path))?,
        None => Vec::new(),
    };
    let criteria = match obj.get("criteria") {
        Some(v) => parse_criteria_list(v, &format!("{}/criteria", path))?,
        None => Vec::new(),
    };
    let extensions = extract_extensions(obj);
    Ok(FailureAction {
        name,
        action_type,
        target,
        parameters,
        retry_after,
        retry_limit,
        criteria,
        extensions,
    })
}

/// Executes `parse_action_target` (parse action target).
///
/// # Arguments
/// * `obj` - Input `obj` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<ActionTarget>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_action_target(obj: &Map<String, JsonValue>, path: &str) -> AppResult<ActionTarget> {
    let wf_id = obj.get("workflowId").and_then(JsonValue::as_str);
    let st_id = obj.get("stepId").and_then(JsonValue::as_str);
    match (wf_id, st_id) {
        (Some(_), Some(_)) => Err(AppError::ArazzoValidation {
            path: path.to_string(),
            reason: "Action cannot specify both 'workflowId' and 'stepId'".to_string(),
        }),
        (Some(w), None) => Ok(ActionTarget::WorkflowId(w.to_string())),
        (None, Some(s)) => Ok(ActionTarget::StepId(s.to_string())),
        (None, None) => Ok(ActionTarget::None),
    }
}

/// Executes `parse_outputs_map` (parse outputs map).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<IndexMap<String, RuntimeExpressionOrSelector>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_outputs_map(
    val: &JsonValue,
    path: &str,
) -> AppResult<IndexMap<String, RuntimeExpressionOrSelector>> {
    let obj = expect_object(val, path, "'outputs' must be an object")?;
    let mut out = IndexMap::new();
    for (k, v) in obj {
        let o_path = format!("{}/{}", path, k);
        if let Some(s) = v.as_str() {
            let expr = RuntimeExpression::parse(s)?;
            out.insert(k.clone(), RuntimeExpressionOrSelector::Expression(expr));
        } else if let Some(s_obj) = v.as_object() {
            let ctx_str = get_required_str(s_obj, "context", "SelectorObject")?;
            let context = RuntimeExpression::parse(ctx_str)?;
            let selector = get_required_str(s_obj, "selector", "SelectorObject")?.to_string();
            let selector_type = match s_obj.get("type") {
                Some(t) => Some(parse_criterion_type(t, &format!("{}/type", o_path))?),
                None => None,
            };
            let extensions = extract_extensions(s_obj);
            out.insert(
                k.clone(),
                RuntimeExpressionOrSelector::Selector(SelectorObject {
                    context,
                    selector,
                    selector_type,
                    extensions,
                }),
            );
        } else {
            return Err(AppError::ArazzoValidation {
                path: o_path,
                reason: "output entry must be a runtime expression string or SelectorObject"
                    .to_string(),
            });
        }
    }
    Ok(out)
}

/// Executes `parse_string_list` (parse string list).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<Vec<String>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_string_list(val: Option<&JsonValue>, path: &str) -> AppResult<Vec<String>> {
    let Some(v) = val else {
        return Ok(Vec::new());
    };
    let arr = expect_array(v, path, "expected an array of strings")?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let Some(s) = item.as_str() else {
            return Err(AppError::ArazzoValidation {
                path: path.to_string(),
                reason: "array elements must be strings".to_string(),
            });
        };
        out.push(s.to_string());
    }
    Ok(out)
}

/// Executes `expect_object` (expect object).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `path` - Input `path` parameter.
/// * `reason` - Input `reason` parameter.
///
/// # Returns
/// Resulting `AppResult<&'a Map<String, JsonValue>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn expect_object<'a>(
    val: &'a JsonValue,
    path: &str,
    reason: &str,
) -> AppResult<&'a Map<String, JsonValue>> {
    val.as_object().ok_or_else(|| AppError::ArazzoValidation {
        path: path.to_string(),
        reason: reason.to_string(),
    })
}

/// Executes `expect_array` (expect array).
///
/// # Arguments
/// * `val` - Input `val` parameter.
/// * `path` - Input `path` parameter.
/// * `reason` - Input `reason` parameter.
///
/// # Returns
/// Resulting `AppResult<&'a Vec<JsonValue>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn expect_array<'a>(val: &'a JsonValue, path: &str, reason: &str) -> AppResult<&'a Vec<JsonValue>> {
    val.as_array().ok_or_else(|| AppError::ArazzoValidation {
        path: path.to_string(),
        reason: reason.to_string(),
    })
}

/// Executes `get_required_val` (get required val).
///
/// # Arguments
/// * `obj` - Input `obj` parameter.
/// * `field` - Input `field` parameter.
/// * `container` - Input `container` parameter.
///
/// # Returns
/// Resulting `AppResult<&'a JsonValue>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn get_required_val<'a>(
    obj: &'a Map<String, JsonValue>,
    field: &str,
    container: &str,
) -> AppResult<&'a JsonValue> {
    match obj.get(field) {
        Some(v) => Ok(v),
        None => Err(AppError::MissingRequiredField {
            container: container.to_string(),
            field: field.to_string(),
        }),
    }
}

/// Executes `get_required_str` (get required str).
///
/// # Arguments
/// * `obj` - Input `obj` parameter.
/// * `field` - Input `field` parameter.
/// * `container` - Input `container` parameter.
///
/// # Returns
/// Resulting `AppResult<&'a str>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn get_required_str<'a>(
    obj: &'a Map<String, JsonValue>,
    field: &str,
    container: &str,
) -> AppResult<&'a str> {
    match get_required_val(obj, field, container)?.as_str() {
        Some(s) => Ok(s),
        None => Err(AppError::MissingRequiredField {
            container: container.to_string(),
            field: field.to_string(),
        }),
    }
}

/// Executes `extract_extensions` (extract extensions).
///
/// # Arguments
/// * `obj` - Input `obj` parameter.
///
/// # Returns
/// Resulting `BTreeMap<String, JsonValue>` value.
fn extract_extensions(obj: &Map<String, JsonValue>) -> BTreeMap<String, JsonValue> {
    let mut ext = BTreeMap::new();
    for (k, v) in obj {
        if k.starts_with("x-") && k.len() > 2 {
            ext.insert(k.clone(), v.clone());
        }
    }
    ext
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_full_arazzo_1_1_0_document() {
        let yaml = r##"
arazzo: 1.1.0
$self: https://example.com/workflows/petstore.arazzo.yaml
x-root-ext: true
info:
  title: Petstore Order Workflow
  summary: End to end pet purchase
  description: Authenticate, find pet, and place order
  version: 1.0.0
  x-info-ext: 1
sourceDescriptions:
  - name: petstore
    url: ./petstore.openapi.yaml
    type: openapi
    x-src-ext: "yes"
  - name: events
    url: ./events.asyncapi.yaml
    type: asyncapi
components:
  x-comp-ext: true
  inputs:
    CustomerInput:
      type: object
      required: [customerId, active]
      properties:
        customerId:
          type: string
          format: uuid
          description: Unique customer ID
        active:
          type: boolean
        tags:
          type: array
          items:
            type: string
        score:
          type: number
          format: float
        ratio:
          type: number
        count:
          type: integer
          format: int32
        total:
          type: integer
        createdAt:
          type: string
          format: date-time
        birthDate:
          type: string
          format: date
        metadata:
          type: object
  parameters:
    traceHeader:
      name: X-Trace-Id
      in: header
      value: $inputs.customerId
      x-param-ext: 1
  successActions:
    finishEarly:
      name: done
      type: end
  failureActions:
    retryDefault:
      name: retryStep
      type: retry
      retryAfter: 1.5
      retryLimit: 3
workflows:
  - workflowId: prereqAuth
    steps:
      - stepId: doAuth
        operationId: petstore.loginUser
        outputs:
          jwt: $response.body#/token
  - workflowId: purchasePet
    summary: Purchase a pet
    description: Complete workflow
    dependsOn: [prereqAuth]
    inputs:
      $ref: "#/components/inputs/CustomerInput"
    parameters:
      - reference: $components.parameters.traceHeader
        value: "override-trace"
    successActions:
      - reference: $components.successActions.finishEarly
    failureActions:
      - reference: $components.failureActions.retryDefault
    outputs:
      finalOrderId: $steps.placeOrder.outputs.orderId
      extractedStatus:
        context: $response.body
        selector: $.status
        type:
          type: jsonpath
          version: rfc9535
    steps:
      - stepId: findPet
        description: Find available pets
        operationPath: "{$sourceDescriptions.petstore.url}#/paths/~1pet~1findByStatus/get"
        timeout: 5000
        parameters:
          - name: status
            in: query
            value: available
        successCriteria:
          - condition: $statusCode == 200
            type: simple
          - context: $response.body
            condition: "^available$"
            type: regex
          - context: $response.body
            condition: "$[0].id"
            type:
              type: jsonpath
              version: rfc9535
        onSuccess:
          - name: gotoOrder
            type: goto
            stepId: placeOrder
            parameters:
              - name: petId
                in: query
                value: $response.body#/0/id
        onFailure:
          - name: retryFind
            type: retry
            retryAfter: 2.0
            retryLimit: 2
            parameters:
              - name: retryParam
                in: query
                value: "1"
        outputs:
          petId: $response.body#/0/id
      - stepId: placeOrder
        dependsOn: [findPet]
        operationId: placeOrder
        requestBody:
          contentType: application/json
          payload:
            petId: 0
            quantity: 1
          replacements:
            - target: /petId
              targetSelectorType:
                type: jsonpointer
                version: rfc6901
              value: $steps.findPet.outputs.petId
        outputs:
          orderId:
            context: $response.body
            selector: /id
            type:
              type: jsonpointer
              version: rfc6901
      - stepId: notifyEvent
        dependsOn: [placeOrder]
        channelPath: /orders/created
        action: send
        correlationId: $steps.placeOrder.outputs.orderId
        requestBody:
          payload: "{$steps.placeOrder.outputs.orderId}"
      - stepId: subFlow
        dependsOn: [notifyEvent]
        workflowId: prereqAuth
"##;

        let doc = parse_arazzo_document(yaml).expect("valid arazzo 1.1.0");
        assert_eq!(doc.arazzo, ArazzoVersion::V1_1_0);
        assert_eq!(
            doc.self_uri.as_deref(),
            Some("https://example.com/workflows/petstore.arazzo.yaml")
        );
        assert_eq!(doc.source_descriptions.len(), 2);
        assert_eq!(doc.workflows.len(), 2);
        assert_eq!(doc.workflows[1].steps.len(), 4);
        assert_eq!(
            topological_sort_workflows(&doc.workflows).expect("wf sort"),
            vec!["prereqAuth".to_string(), "purchasePet".to_string()]
        );
        assert_eq!(
            topological_sort_steps(&doc.workflows[1]).expect("step sort"),
            vec![
                "findPet".to_string(),
                "placeOrder".to_string(),
                "notifyEvent".to_string(),
                "subFlow".to_string()
            ]
        );
    }

    #[test]
    fn test_resolve_source_descriptions_and_step_operations() {
        let dir = std::env::temp_dir().join("cdd_arazzo_resolve_test");
        let _ = std::fs::create_dir_all(&dir);
        let oas_path = dir.join("petstore.yaml");
        std::fs::write(
            &oas_path,
            r#"
openapi: 3.2.0
info:
  title: Petstore
  version: 1.0.0
paths:
  /pets:
    post:
      operationId: createPet
      responses:
        "200":
          description: ok
"#,
        )
        .expect("write oas");

        let arazzo_yaml = r##"
arazzo: 1.1.0
$self: ./workflows.yaml
info:
  title: Test
  version: 1.0.0
sourceDescriptions:
  - name: petstore
    url: petstore.yaml
    type: openapi
  - name: events
    url: events.yaml
    type: asyncapi
workflows:
  - workflowId: wf1
    steps:
      - stepId: s1
        operationId: petstore.createPet
      - stepId: s2
        operationId: createPet
      - stepId: s3
        operationPath: "{$sourceDescriptions.petstore.url}#/paths/~1pets/post"
      - stepId: s4
        workflowId: wf1
"##;
        let base_url = Url::parse("https://example.com/specs/").expect("base");
        let mut reg = DocumentRegistry::new();
        let doc = parse_arazzo_document_with_registry(arazzo_yaml, Some(&base_url), &mut reg)
            .expect("doc");
        assert_eq!(
            doc.self_uri.as_deref(),
            Some("https://example.com/specs/workflows.yaml")
        );

        let empty_linked = resolve_source_descriptions(&doc, None, &mut reg).expect("none dir");
        assert!(empty_linked.is_empty());

        let mut dir_doc = doc.clone();
        dir_doc.source_descriptions.push(SourceDescription {
            name: "dir_src".to_string(),
            url: ".".to_string(),
            source_type: Some(SourceDescriptionType::OpenApi),
            extensions: BTreeMap::new(),
        });
        let mut dir_reg = DocumentRegistry::new();
        let _ = resolve_source_descriptions(&dir_doc, Some(&dir), &mut dir_reg).expect("dir entry");

        let linked = resolve_source_descriptions(&doc, Some(&dir), &mut reg).expect("linked");
        assert!(linked.contains_key("petstore"));

        for step in doc.workflows[0].steps.iter().take(3) {
            let route = resolve_step_operation(step, &linked).expect("route");
            assert_eq!(route.path, "/pets");
            assert_eq!(route.method, "POST");
        }
        assert!(resolve_step_operation(&doc.workflows[0].steps[3], &linked).is_none());

        let unknown_step = WorkflowStep {
            step_id: "u".to_string(),
            description: None,
            target: StepTarget::OperationId("missingOp".to_string()),
            parameters: vec![],
            request_body: None,
            success_criteria: vec![],
            on_success: vec![],
            on_failure: vec![],
            outputs: IndexMap::new(),
            timeout: None,
            correlation_id: None,
            action: None,
            depends_on: vec![],
            extensions: BTreeMap::new(),
        };
        assert!(resolve_step_operation(&unknown_step, &linked).is_none());

        let unknown_src_step = WorkflowStep {
            target: StepTarget::OperationId("$sourceDescriptions.unknown.createPet".to_string()),
            ..unknown_step.clone()
        };
        assert!(resolve_step_operation(&unknown_src_step, &linked).is_none());

        let unknown_op_in_src_step = WorkflowStep {
            target: StepTarget::OperationId("$sourceDescriptions.petstore.missingOp".to_string()),
            ..unknown_step.clone()
        };
        assert!(resolve_step_operation(&unknown_op_in_src_step, &linked).is_none());

        let short_ptr_step = WorkflowStep {
            target: StepTarget::OperationPath(
                "{$sourceDescriptions.petstore.url}#/paths".to_string(),
            ),
            ..unknown_step.clone()
        };
        assert!(resolve_step_operation(&short_ptr_step, &linked).is_none());

        let non_paths_ptr_step = WorkflowStep {
            target: StepTarget::OperationPath(
                "{$sourceDescriptions.petstore.url}#/webhooks/newPet/post".to_string(),
            ),
            ..unknown_step.clone()
        };
        assert!(resolve_step_operation(&non_paths_ptr_step, &linked).is_none());

        let wrong_method_step = WorkflowStep {
            target: StepTarget::OperationPath(
                "{$sourceDescriptions.petstore.url}#/paths/~1pets/delete".to_string(),
            ),
            ..unknown_step.clone()
        };
        assert!(resolve_step_operation(&wrong_method_step, &linked).is_none());

        let unknown_path_step = WorkflowStep {
            target: StepTarget::OperationPath(
                "{$sourceDescriptions.petstore.url}#/paths/~1missing/get".to_string(),
            ),
            ..unknown_step
        };
        assert!(resolve_step_operation(&unknown_path_step, &linked).is_none());

        // Test invalid OpenAPI file in resolve_source_descriptions
        std::fs::write(&oas_path, "not_openapi: [unclosed").expect("write bad");
        assert!(resolve_source_descriptions(&doc, Some(&dir), &mut reg).is_err());

        std::fs::write(
            &oas_path,
            "openapi: 3.2.0
info:
  title: T
",
        )
        .expect("write incomplete oas");
        assert!(resolve_source_descriptions(&doc, Some(&dir), &mut reg).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_validation_error_branches() {
        let edge_valid = r#"
arazzo: 1.1.0
x-: ignored_bare_ext
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    inputs:
      $ref: '#/definitions/External'
      required: [123]
    steps:
      - stepId: s1
        operationId: op1
"#;
        assert!(parse_arazzo_document(edge_valid).is_ok());

        let invalid_cases = [
            "not_an_object",
            "{}",
            "arazzo: 123",
            "arazzo: 2.0.0",
            "arazzo: 1.1.0\ninfo:\n  title: T\n  version: '1'\nsourceDescriptions:\n  - name: s1\n    url: ./a.yaml\nworkflows:\n  - workflowId: w1\n    dependsOn: [123]\n    steps:\n      - stepId: s1\n        operationId: op1",
            "arazzo: 1.1.0\ninfo:\n  title: T\n  version: '1'\nsourceDescriptions:\n  - name: ''\n    url: ./a.yaml\nworkflows: []",
            "arazzo: 1.1.0\ninfo:\n  title: T\n  version: '1'\nsourceDescriptions:\n  - name: s1\n    url: ./a.yaml\ncomponents: {}\nworkflows:\n  - workflowId: w1\n    inputs:\n      $ref: '#/components/inputs/Missing'\n    steps:\n      - stepId: s1\n        operationId: op1",
            "arazzo: 1.1.0
$self: 123",
            "arazzo: 1.1.0
$self: ':::bad_uri'",
            "arazzo: 1.1.0
info: 123",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions: 123",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions: [123]",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions: []
workflows: []",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: 'bad name!'
    url: ./a.yaml
workflows: []",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
  - name: s1
    url: ./b.yaml
workflows: []",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows: []",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
components: 123
workflows: []",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows: 123",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows: [123]",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: '   '
    steps: []",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    steps: []",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    steps:
      - stepId: s1
        operationId: op1
  - workflowId: w1
    steps:
      - stepId: s1
        operationId: op1",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    steps:
      - stepId: '  '
        operationId: op1",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    steps:
      - stepId: s1
        operationId: op1
      - stepId: s1
        operationId: op2",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    steps:
      - stepId: s1",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    steps:
      - stepId: s1
        operationId: op1
        workflowId: w2",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    dependsOn: [missingWf]
    steps:
      - stepId: s1
        operationId: op1",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    dependsOn: [w2]
    steps:
      - stepId: s1
        operationId: op1
  - workflowId: w2
    dependsOn: [w1]
    steps:
      - stepId: s1
        operationId: op1",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    steps:
      - stepId: s1
        dependsOn: [missingStep]
        operationId: op1",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    steps:
      - stepId: s1
        dependsOn: [s2]
        operationId: op1
      - stepId: s2
        dependsOn: [s1]
        operationId: op2",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    inputs:
      $ref: '#/components/inputs/Missing'
    steps:
      - stepId: s1
        operationId: op1",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    parameters:
      - reference: $components.parameters.missing
    steps:
      - stepId: s1
        operationId: op1",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    parameters:
      - reference: $inputs.wrongSection
    steps:
      - stepId: s1
        operationId: op1",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    successActions:
      - reference: $components.successActions.missing
    steps:
      - stepId: s1
        operationId: op1",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    successActions:
      - reference: $inputs.wrong
    steps:
      - stepId: s1
        operationId: op1",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    failureActions:
      - reference: $components.failureActions.missing
    steps:
      - stepId: s1
        operationId: op1",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    failureActions:
      - reference: $inputs.wrong
    steps:
      - stepId: s1
        operationId: op1",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    steps:
      - stepId: s1
        operationId: op1
        onSuccess:
          - name: badGoto
            type: goto",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    steps:
      - stepId: s1
        operationId: op1
        onFailure:
          - name: badGoto
            type: goto",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    steps:
      - stepId: s1
        operationId: op1
        onFailure:
          - name: badTarget
            type: goto
            workflowId: w1
            stepId: s1",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    steps:
      - stepId: s1
        operationId: op1
        onFailure:
          - name: negRetry
            type: retry
            retryAfter: -1.0",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    steps:
      - stepId: s1
        operationId: op1
        outputs:
          badOut: 123",
            "arazzo: 1.1.0
info:
  title: T
  version: '1'
sourceDescriptions:
  - name: s1
    url: ./a.yaml
workflows:
  - workflowId: w1
    steps:
      - stepId: s1
        operationId: op1
        successCriteria:
          - condition: '$statusCode == 200'
            type: 123",
        ];

        for yaml in invalid_cases {
            assert!(
                parse_arazzo_document(yaml).is_err(),
                "Expected error for:
{}",
                yaml
            );
        }
    }
}
