#![deny(missing_docs)]

//! # Arazzo 1.1.0 Parsing Module
//!
//! Provides parsers for Arazzo 1.1.0 YAML/JSON specifications, runtime expressions,
//! and Rust AST workflow extraction.

/// YAML/JSON Arazzo document parser, DAG validator, and cross-spec resolver.
pub mod document;
/// Arazzo 1.1.0 runtime expression AST classifier and Rust expression emitter.
pub mod expressions;
/// Static Rust AST workflow extractor (`Rust -> Arazzo`).
pub mod rust_ast;

pub use document::{
    parse_arazzo_document, parse_arazzo_document_with_registry, resolve_source_descriptions,
    resolve_step_operation, topological_sort_steps, topological_sort_workflows,
    validate_arazzo_document,
};
pub use expressions::{
    runtime_expression_to_rust, ArazzoExpressionRoot, AsyncMessageSource, ComponentSection,
    HttpMessageSource,
};
pub use rust_ast::{extract_arazzo_document_from_rust, extract_workflows_from_rust};
