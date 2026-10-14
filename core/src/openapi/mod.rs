#![deny(missing_docs)]

//! OpenAPI Module
//!
//! Parses OpenAPI 2.0 / 3.0 / 3.1 / 3.2 specifications into IR and emits OpenAPI 3.2 / JSON Schema documents.

/// OpenAPI 3.2 and JSON Schema emitter.
pub mod emit;
/// OpenAPI and Swagger specification parser and reference resolver.
pub mod parse;
