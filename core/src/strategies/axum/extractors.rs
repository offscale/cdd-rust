#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Extractors
//!
//! Generates the Axum extractor types for path, query, and bodies.

use crate::openapi::parse::models::{ContentMediaType, SecurityRequirementGroup};

/// Path extractor.
pub fn path_extractor(inner_types: &[String]) -> String {
    if inner_types.len() == 1 {
        format!("Path<{}>", inner_types[0])
    } else {
        let tuple_inner = inner_types.join(", ");
        format!("Path<({})>", tuple_inner)
    }
}

/// Query extractor.
pub fn query_extractor() -> String {
    "Query<serde_json::Value>".to_string()
}

/// Typed Query extractor.
pub fn typed_query_extractor(inner_type: &str) -> String {
    format!("Query<{}>", inner_type)
}

/// Query String extractor.
pub fn query_string_extractor(
    inner_type: &str,
    _content_media_type: Option<&ContentMediaType>,
) -> String {
    format!("Query<{}>", inner_type)
}

/// Header extractor.
pub fn header_extractor(_inner_type: &str) -> String {
    // Axum has HeaderMap or TypedHeader
    "axum::http::HeaderMap".to_string()
}

/// Cookie extractor.
pub fn cookie_extractor() -> String {
    "axum_extra::extract::cookie::CookieJar".to_string() // Typical in axum
}

/// Body extractor (JSON).
pub fn body_extractor(body_type: &str) -> String {
    if body_type == "serde_json::Value" {
        "Json<serde_json::Value>".to_string()
    } else {
        format!("Json<{}>", body_type)
    }
}

/// Form extractor.
pub fn form_extractor(body_type: &str) -> String {
    format!("Form<{}>", body_type)
}

/// Multipart form extractor.
pub fn multipart_extractor(_body_type: &str) -> String {
    "Multipart".to_string()
}

/// Text extractor.
pub fn text_extractor(_body_type: &str) -> String {
    "String".to_string()
}

/// Bytes extractor.
pub fn bytes_extractor(_body_type: &str) -> String {
    "axum::body::Bytes".to_string()
}

/// Security extractor.
pub fn security_extractor(_requirements: &[SecurityRequirementGroup]) -> String {
    // Basic placeholder for security
    "Extension<AuthUser>".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_extractor() {
        assert_eq!(path_extractor(&["i32".to_string()]), "Path<i32>");
        assert_eq!(
            path_extractor(&["i32".to_string(), "String".to_string()]),
            "Path<(i32, String)>"
        );
    }

    #[test]
    fn test_extractors() {
        assert_eq!(body_extractor("MyBody"), "Json<MyBody>");
        assert_eq!(query_extractor(), "Query<serde_json::Value>");
    }
}
