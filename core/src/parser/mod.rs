#![deny(missing_docs)]

//! # Parser Module
//!
//! Handles parsing of Rust source code using the rust-analyzer syntax library.
//! Extracts structs, enums, fields, documentation, and specific attributes (serde/oai).

pub mod attributes;
pub mod extractors;
pub mod models;

// Re-export major types and functions to maintain API compatibility
pub use extractors::{extract_model, extract_struct, extract_struct_fields, extract_struct_names};
pub use models::{
    ParsedEnum, ParsedExternalDocs, ParsedField, ParsedModel, ParsedStruct, ParsedVariant,
};
