#![deny(missing_docs)]

//! # OpenAPI Parsing Module
//!
//! - **models**: Intermediate Representation definitions.
//! - **resolver**: Logic for mapping OpenAPI types to Rust types.
//! - **routes**: Parsing logic for API paths/endpoints.
//! - **schemas**: Parsing logic for data model definitions.
//! - **validation**: Spec-level structural validations for OpenAPI documents.

pub mod models;
pub(crate) mod ref_utils;
pub mod resolver;
pub mod routes;
pub mod schemas;
pub(crate) mod validation;

// Re-export public API to maintain compatibility
pub use models::{
    BodyFormat, ContentMediaType, ParamSource, ParsedRoute, RequestBodyDefinition, RouteParam,
};
pub use routes::parse_openapi_routes;
pub use schemas::parse_openapi_spec;
