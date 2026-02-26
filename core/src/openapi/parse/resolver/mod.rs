#![deny(missing_docs)]

//! # Resolver Module
//!
//! Logic for resolving OpenAPI Schema definitions into Rust types.
//!
//! Handles:
//! - Recursive type mapping.
//! - Parameter resolution (Inline and Reference) via `ShimParameter`.
//! - Parsing of parameter styles, explode rules, and reserved character rules (OpenAPI 3.2.0 compliant).
//! - Default logic based on parameter location (Query, Path, Header, Cookie).
//! - Swagger 2.0 `collectionFormat` compatibility mapping.

pub mod body;
pub mod params;
pub mod responses;
pub mod types;

// Re-export public members to maintain API compatibility
pub use body::{extract_request_body_raw, extract_request_body_raw_with_registry};
pub use body::{extract_request_body_type, extract_request_body_type_with_registry};
pub use params::{resolve_parameters, resolve_parameters_with_registry, ShimParameter};
pub use responses::{extract_response_details, extract_response_details_with_registry};
pub use types::map_schema_to_rust_type;
