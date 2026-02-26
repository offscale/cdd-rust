#![deny(missing_docs)]

//! # OpenAPI Parsing Module
//!
//! - **models**: Intermediate Representation definitions.
//! - **resolver**: Logic for mapping OpenAPI types to Rust types.
//! - **routes**: Parsing logic for API paths/endpoints.
//! - **schemas**: Parsing logic for data model definitions.
//! - **validation**: Spec-level structural validations for OpenAPI documents.

pub mod document;
pub mod models;
pub(crate) mod normalization;
pub(crate) mod ref_utils;
pub mod registry;
pub mod resolver;
pub mod routes;
pub mod schemas;
pub(crate) mod validation;

// Re-export public API to maintain compatibility
pub use document::{parse_openapi_document, parse_openapi_document_with_registry, ParsedOpenApi};
pub use models::{
    BodyFormat, ContentMediaType, ExampleKind, ExampleValue, ParamSource, ParsedRoute,
    RequestBodyDefinition, RouteParam,
};
pub use registry::DocumentRegistry;
pub use routes::{parse_openapi_routes, parse_openapi_routes_with_registry};
pub use schemas::{parse_openapi_spec, parse_openapi_spec_with_registry};
