#![deny(missing_docs)]

//! # Arazzo 1.1.0 Specification Module
//!
//! Provides strongly typed Intermediate Representation (IR) models, runtime expression
//! parsing, YAML/JSON document parsing, Rust AST workflow extraction, and Arazzo 1.1.0
//! specification emission.

/// Canonical Arazzo 1.1.0 JSON/YAML specification emitter.
pub mod emit;
/// Strongly typed Arazzo 1.1.0 IR data structures.
pub mod models;
/// Arazzo 1.1.0 specification and runtime expression parsers.
pub mod parse;

pub use emit::{generate_arazzo_document, serialize_arazzo_document, ArazzoFormat};
pub use models::{
    ActionOrReusable, ActionTarget, ArazzoComponents, ArazzoDocument, ArazzoInfo,
    ArazzoParamLocation, ArazzoParameter, ArazzoRequestBody, ArazzoVersion, AsyncActionType,
    Criterion, CriterionExpressionType, CriterionType, ExpressionDialect, FailureAction,
    FailureActionType, ParameterOrReusable, ParameterValue, ParsedSchema, PayloadReplacement,
    ReusableObject, RuntimeExpressionOrSelector, SelectorObject, SourceDescription,
    SourceDescriptionType, StepTarget, SuccessAction, SuccessActionType, Workflow, WorkflowStep,
};
pub use parse::{
    extract_arazzo_document_from_rust, extract_workflows_from_rust, parse_arazzo_document,
    parse_arazzo_document_with_registry, resolve_source_descriptions, resolve_step_operation,
    runtime_expression_to_rust, topological_sort_steps, topological_sort_workflows,
    validate_arazzo_document,
};
