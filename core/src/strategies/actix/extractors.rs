#![deny(missing_docs)]

//! # Extractors
//!
//! Logic for generating Rust type strings that Actix uses to extract data
//! from requests (Path, Query, Json, etc.).

use crate::oas::models::SecurityRequirement;
use crate::strategies::actix::utils::to_pascal_case;

/// Generates the type string for path parameters.
pub fn path_extractor(inner_types: &[String]) -> String {
    if inner_types.len() == 1 {
        format!("web::Path<{}>", inner_types[0])
    } else {
        let tuple = inner_types.join(", ");
        format!("web::Path<({})>", tuple)
    }
}

/// Generates the type string for query extraction.
pub fn query_extractor() -> String {
    "web::Query<Value>".to_string()
}

/// Generates the type string for strict Query String extraction (OAS 3.2).
pub fn query_string_extractor(inner_type: &str) -> String {
    format!("web::Query<{}>", inner_type)
}

/// Generates the type string for header extraction.
pub fn header_extractor(_inner_type: &str) -> String {
    "web::Header<String>".to_string()
}

/// Generates the type string for cookie extraction.
pub fn cookie_extractor() -> String {
    "web::Cookie".to_string()
}

/// Generates the type string for JSON body extraction.
pub fn body_extractor(body_type: &str) -> String {
    format!("web::Json<{}>", body_type)
}

/// Generates the type string for Form body extraction.
pub fn form_extractor(body_type: &str) -> String {
    format!("web::Form<{}>", body_type)
}

/// Generates the type string for Multipart extraction.
pub fn multipart_extractor() -> String {
    "Multipart".to_string()
}

/// Generates the type string for Security extraction (Guard/ReqData).
pub fn security_extractor(requirements: &[SecurityRequirement]) -> String {
    if requirements.is_empty() {
        return "".to_string();
    }

    let req = &requirements[0];
    let scheme = to_pascal_case(&req.scheme_name);

    if req.scopes.is_empty() {
        format!("_auth: web::ReqData<security::{}>", scheme)
    } else {
        let normalized_scopes: Vec<String> = req
            .scopes
            .iter()
            .map(|s| format!("security::scopes::{}", to_pascal_case(s)))
            .collect();

        let scopes_tuple = if normalized_scopes.len() == 1 {
            normalized_scopes[0].clone()
        } else {
            format!("({})", normalized_scopes.join(", "))
        };

        format!(
            "_auth: web::ReqData<security::Authenticated<security::{}, {}>>",
            scheme, scopes_tuple
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extractors() {
        assert_eq!(path_extractor(&["Uuid".into()]), "web::Path<Uuid>");
        assert_eq!(query_string_extractor("Filter"), "web::Query<Filter>");
        assert_eq!(body_extractor("Dto"), "web::Json<Dto>");
    }
}
