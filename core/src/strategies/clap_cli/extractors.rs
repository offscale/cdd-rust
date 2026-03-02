#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Extractors for Reqwest Client
//!
//! Maps OpenAPI parameter and body definitions to plain Rust types
//! used in client method signatures.

use crate::openapi::parse::models::{ContentMediaType, SecurityRequirementGroup};

/// Generates the type string for path parameters.
pub fn path_extractor(inner_types: &[String]) -> String {
    if inner_types.len() == 1 {
        inner_types[0].clone()
    } else {
        let tuple = inner_types.join(", ");
        format!("({})", tuple)
    }
}

/// Generates the type string for untyped query extraction.
pub fn query_extractor() -> String {
    "serde_json::Value".to_string()
}

/// Generates the type string for strongly typed query extraction.
pub fn typed_query_extractor(inner_type: &str) -> String {
    inner_type.to_string()
}

/// Generates the type string for raw query string extraction.
pub fn query_string_extractor(
    _inner_type: &str,
    _content_media_type: Option<&ContentMediaType>,
) -> String {
    "String".to_string()
}

/// Generates the type string for header extraction.
pub fn header_extractor(_inner_type: &str) -> String {
    "String".to_string()
}

/// Generates the type string for cookie extraction.
pub fn cookie_extractor() -> String {
    "String".to_string()
}

/// Generates the type string for JSON body extraction.
pub fn body_extractor(body_type: &str) -> String {
    body_type.to_string()
}

/// Generates the type string for Form body extraction.
pub fn form_extractor(body_type: &str) -> String {
    body_type.to_string()
}

/// Generates the type string for Multipart extraction.
pub fn multipart_extractor(body_type: &str) -> String {
    if body_type == "Multipart" {
        "reqwest::multipart::Form".to_string()
    } else {
        body_type.to_string()
    }
}

/// Generates the type string for Text body extraction.
pub fn text_extractor(_body_type: &str) -> String {
    "String".to_string()
}

/// Generates the type string for Binary body extraction.
pub fn bytes_extractor(_body_type: &str) -> String {
    "Vec<u8>".to_string()
}

/// Generates the type string for Security extraction.
pub fn security_extractor(_requirements: &[SecurityRequirementGroup]) -> String {
    "".to_string()
}
