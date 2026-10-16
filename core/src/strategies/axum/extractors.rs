#![deny(missing_docs)]

//! # Extractors
//!
//! Generates the Axum extractor types for path, query, headers, cookies, security, and bodies.

use crate::openapi::parse::models::{ContentMediaType, SecurityRequirementGroup};

/// Path extractor.
///
/// # Arguments
/// * `inner_types` - The Rust types of the path parameters.
///
/// # Returns
/// The Axum `Path<...>` extractor string.
pub fn path_extractor(inner_types: &[String]) -> String {
    if inner_types.len() == 1 {
        format!("Path<{}>", inner_types[0])
    } else {
        let tuple_inner = inner_types.join(", ");
        format!("Path<({})>", tuple_inner)
    }
}

/// Query extractor.
///
/// # Returns
/// The Axum `Query<serde_json::Value>` extractor string.
pub fn query_extractor() -> String {
    "Query<serde_json::Value>".to_string()
}

/// Typed Query extractor.
///
/// # Arguments
/// * `inner_type` - The Rust struct type representing query parameters.
///
/// # Returns
/// The Axum `Query<T>` extractor string.
pub fn typed_query_extractor(inner_type: &str) -> String {
    format!("Query<{}>", inner_type)
}

/// Query String extractor.
///
/// # Arguments
/// * `inner_type` - The specific type the entire query string maps to.
/// * `_content_media_type` - Media type used to serialize the query string.
///
/// # Returns
/// The Axum `Query<T>` extractor string.
pub fn query_string_extractor(
    inner_type: &str,
    _content_media_type: Option<&ContentMediaType>,
) -> String {
    format!("Query<{}>", inner_type)
}

/// Header extractor using the parameter's actual inner type when specific,
/// or `axum::http::HeaderMap` for generic/untyped header maps.
///
/// # Arguments
/// * `inner_type` - The Rust type of the header parameter.
///
/// # Returns
/// The Axum header extractor string.
pub fn header_extractor(inner_type: &str) -> String {
    let trimmed = inner_type.trim();
    if trimmed.is_empty() || trimmed == "HeaderMap" || trimmed == "axum::http::HeaderMap" {
        "axum::http::HeaderMap".to_string()
    } else {
        format!("axum_extra::TypedHeader<{}>", trimmed)
    }
}

/// Cookie extractor.
///
/// # Returns
/// The Axum `CookieJar` extractor string.
pub fn cookie_extractor() -> String {
    "axum_extra::extract::cookie::CookieJar".to_string()
}

/// Body extractor (JSON).
///
/// # Arguments
/// * `body_type` - The Rust type of the JSON request body.
///
/// # Returns
/// The Axum `Json<T>` extractor string.
pub fn body_extractor(body_type: &str) -> String {
    if body_type == "serde_json::Value" {
        "Json<serde_json::Value>".to_string()
    } else {
        format!("Json<{}>", body_type)
    }
}

/// Form extractor.
///
/// # Arguments
/// * `body_type` - The Rust type of the form request body.
///
/// # Returns
/// The Axum `Form<T>` extractor string.
pub fn form_extractor(body_type: &str) -> String {
    format!("Form<{}>", body_type)
}

/// Multipart form extractor.
///
/// # Arguments
/// * `_body_type` - The Rust type of the multipart request body.
///
/// # Returns
/// The Axum `Multipart` extractor string.
pub fn multipart_extractor(_body_type: &str) -> String {
    "Multipart".to_string()
}

/// Text extractor.
///
/// # Arguments
/// * `_body_type` - The Rust type of the text request body.
///
/// # Returns
/// The `String` extractor string.
pub fn text_extractor(_body_type: &str) -> String {
    "String".to_string()
}

/// Bytes extractor.
///
/// # Arguments
/// * `_body_type` - The Rust type of the binary request body.
///
/// # Returns
/// The Axum `Bytes` extractor string.
pub fn bytes_extractor(_body_type: &str) -> String {
    "axum::body::Bytes".to_string()
}

/// Security extractor that inspects the operation's [`SecurityRequirementGroup`] schemes
/// and produces a scheme-specific `Extension<...>` guard (or `Option<Extension<...>>` when
/// an anonymous `{}` alternative is present).
///
/// # Arguments
/// * `requirements` - The security requirement groups defined for the route.
///
/// # Returns
/// The Axum `Extension<...>` security extractor string.
pub fn security_extractor(requirements: &[SecurityRequirementGroup]) -> String {
    if requirements.is_empty() {
        return String::new();
    }

    let is_optional = requirements
        .iter()
        .any(SecurityRequirementGroup::is_anonymous);
    let mut scheme_names = Vec::new();
    for group in requirements {
        for scheme in &group.schemes {
            let pascal = scheme
                .scheme_name
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|s| !s.is_empty())
                .map(|part| {
                    let mut chars = part.chars();
                    let first = chars.next().into_iter().flat_map(char::to_uppercase);
                    first.chain(chars).collect::<String>()
                })
                .collect::<String>();
            if !pascal.is_empty() && !scheme_names.contains(&pascal) {
                scheme_names.push(pascal);
            }
        }
    }

    let principal = if scheme_names.is_empty() {
        "AuthUser".to_string()
    } else {
        scheme_names.join("Or")
    };

    if is_optional {
        format!("auth: Option<Extension<{}>>", principal)
    } else {
        format!("auth: Extension<{}>", principal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::SecurityRequirement;

    #[test]
    fn test_path_extractor() {
        assert_eq!(path_extractor(&["i32".to_string()]), "Path<i32>");
        assert_eq!(
            path_extractor(&["i32".to_string(), "String".to_string()]),
            "Path<(i32, String)>"
        );
    }

    #[test]
    fn test_header_and_security_extractors() {
        assert_eq!(header_extractor(""), "axum::http::HeaderMap");
        assert_eq!(header_extractor("HeaderMap"), "axum::http::HeaderMap");
        assert_eq!(
            header_extractor("axum::http::HeaderMap"),
            "axum::http::HeaderMap"
        );
        assert_eq!(
            header_extractor("String"),
            "axum_extra::TypedHeader<String>"
        );

        assert_eq!(security_extractor(&[]), "");

        let req_group = SecurityRequirementGroup {
            schemes: vec![
                SecurityRequirement {
                    scheme_name: "api_key".to_string(),
                    scopes: vec![],
                    scheme: None,
                },
                SecurityRequirement {
                    scheme_name: "api-key".to_string(),
                    scopes: vec![],
                    scheme: None,
                },
                SecurityRequirement {
                    scheme_name: "---".to_string(),
                    scopes: vec![],
                    scheme: None,
                },
            ],
        };
        assert_eq!(
            security_extractor(std::slice::from_ref(&req_group)),
            "auth: Extension<ApiKey>"
        );

        let anon_group = SecurityRequirementGroup { schemes: vec![] };
        assert_eq!(
            security_extractor(&[req_group, anon_group]),
            "auth: Option<Extension<ApiKey>>"
        );
        assert_eq!(
            security_extractor(&[SecurityRequirementGroup { schemes: vec![] }]),
            "auth: Option<Extension<AuthUser>>"
        );
    }

    #[test]
    fn test_extractors() {
        assert_eq!(body_extractor("MyBody"), "Json<MyBody>");
        assert_eq!(
            body_extractor("serde_json::Value"),
            "Json<serde_json::Value>"
        );
        assert_eq!(query_extractor(), "Query<serde_json::Value>");
        assert_eq!(typed_query_extractor("MyQuery"), "Query<MyQuery>");
        assert_eq!(query_string_extractor("MyQuery", None), "Query<MyQuery>");
        assert_eq!(cookie_extractor(), "axum_extra::extract::cookie::CookieJar");
        assert_eq!(form_extractor("MyForm"), "Form<MyForm>");
        assert_eq!(multipart_extractor("MyMultipart"), "Multipart");
        assert_eq!(text_extractor("String"), "String");
        assert_eq!(bytes_extractor("Vec<u8>"), "axum::body::Bytes");
    }
}
