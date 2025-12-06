#![deny(missing_docs)]

//! # OpenAPI Parsing Module
//!
//! - **models**: Intermediate Representation definitions.
//! - **resolver**: Logic for mapping OpenAPI types to Rust types.
//! - **routes**: Parsing logic for API paths/endpoints.
//! - **schemas**: Parsing logic for data model definitions.

pub mod models;
pub mod resolver;
pub mod routes;
pub mod schemas;

// Re-export public API to maintain compatibility
pub use models::{BodyFormat, ParamSource, ParsedRoute, RequestBodyDefinition, RouteParam};
pub use routes::parse_openapi_routes;
pub use schemas::parse_openapi_spec;
