#![deny(missing_docs)]

//! Functions Module
//!
//! Parses and emits HTTP handler and client functions from/to OpenAPI route IR.

/// Handler and client function code emission.
pub mod emit;
/// Actix Web handler function AST parser.
pub mod parse;
/// Reqwest client function AST parser.
pub mod parse_reqwest;
