#![deny(missing_docs)]

//! # Code Patching
//!
//! Utilities for modifying Rust source code strings based on AST analysis.
//!
//! - **structs**: Modifying struct fields, types, and attributes.
//! - **files**: Modifying file-level items like imports.
//! - **workflows**: High-level patching recipes (e.g. OAS injection).

pub(crate) mod common;

/// File-level patching operations (e.g. imports).
pub mod files;

/// Struct-level patching operations (e.g. fields, attributes).
pub mod structs;

/// High-level patching workflows.
pub mod workflows;

// Re-export public API to match original core::patcher interface
pub use files::add_import;
pub use structs::{add_derive, add_struct_attribute, add_struct_field, modify_struct_field_type};
pub use workflows::inject_openapi_attributes;
