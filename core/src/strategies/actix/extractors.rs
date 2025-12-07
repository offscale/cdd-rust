#![deny(missing_docs)]

//! # Extractors
//!
//! Logic for generating Rust type strings that Actix uses to extract data
//! from requests (Path, Query, Json, etc.).

use crate::oas::models::{ParamSource, SecurityRequirement, SecuritySchemeKind};
use crate::strategies::actix::utils::{to_pascal_case, to_snake_case};

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
///
/// If `body_type` is "Multipart" (default fallback for untyped schemas), returns valid generic `actix_multipart::Multipart`.
/// Otherwise returns `actix_multipart::form::MultipartForm<T>` which requires a struct deriving `MultipartForm`.
pub fn multipart_extractor(body_type: &str) -> String {
    if body_type == "Multipart" {
        "actix_multipart::Multipart".to_string()
    } else {
        format!("actix_multipart::form::MultipartForm<{}>", body_type)
    }
}

/// Generates the type string for Security extraction (Guard/ReqData).
///
/// Supported Generation Support:
/// - `Http` (`Bearer`) -> `actix_web_httpauth::extractors::bearer::BearerAuth`
/// - `Http` (`Basic`) -> `actix_web_httpauth::extractors::basic::BasicAuth`
/// - `ApiKey` (`Query`) -> `web::Query<serde_json::Value>` (Validation check logic assumed in body)
/// - Others -> Fallback to `web::ReqData<security::Scheme>` assuming middleware.
pub fn security_extractor(requirements: &[SecurityRequirement]) -> String {
    if requirements.is_empty() {
        return "".to_string();
    }

    let req = &requirements[0];
    let base_name = to_snake_case(&req.scheme_name);

    // Use scheme info if available to generate strict types
    if let Some(info) = &req.scheme {
        match &info.kind {
            SecuritySchemeKind::Http { scheme, .. } => match scheme.to_lowercase().as_str() {
                "bearer" => format!(
                    "{}_auth: actix_web_httpauth::extractors::bearer::BearerAuth",
                    base_name
                ),
                "basic" => format!(
                    "{}_auth: actix_web_httpauth::extractors::basic::BasicAuth",
                    base_name
                ),
                _ => generate_generic_extractor(req),
            },
            SecuritySchemeKind::ApiKey {
                in_loc: ParamSource::Query,
                ..
            } => {
                // ApiKey in Query is just a query param.
                // We extract as generic Value to allow code to check presence.
                format!("{}_key: web::Query<serde_json::Value>", base_name)
            }
            SecuritySchemeKind::ApiKey { .. } => {
                // Header/Cookie requires custom structs for typed extraction in Actix which we can't scaffold here.
                // Fallback to ReqData (implying middleware).
                generate_generic_extractor(req)
            }
            // OAuth/OIDC usually handled by middleware -> ReqData
            _ => generate_generic_extractor(req),
        }
    } else {
        // Fallback for when components are missing or unresolvable
        generate_generic_extractor(req)
    }
}

fn generate_generic_extractor(req: &SecurityRequirement) -> String {
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
    use crate::oas::models::{SecuritySchemeInfo, SecuritySchemeKind};

    #[test]
    fn test_extractors() {
        assert_eq!(path_extractor(&["Uuid".into()]), "web::Path<Uuid>");
        assert_eq!(query_string_extractor("Filter"), "web::Query<Filter>");
        assert_eq!(body_extractor("Dto"), "web::Json<Dto>");
    }

    #[test]
    fn test_multipart_extractor_defaults() {
        // Fallback for untyped
        assert_eq!(
            multipart_extractor("Multipart"),
            "actix_multipart::Multipart"
        );
        // Checked output for typed
        assert_eq!(
            multipart_extractor("UserProfile"),
            "actix_multipart::form::MultipartForm<UserProfile>"
        );
    }

    #[test]
    fn test_security_extractor_bearer() {
        let req = SecurityRequirement {
            scheme_name: "jwt".to_string(),
            scopes: vec![],
            scheme: Some(SecuritySchemeInfo {
                kind: SecuritySchemeKind::Http {
                    scheme: "bearer".into(),
                    bearer_format: Some("JWT".into()),
                },
                description: None,
            }),
        };
        let code = security_extractor(&[req]);
        assert_eq!(
            code,
            "jwt_auth: actix_web_httpauth::extractors::bearer::BearerAuth"
        );
    }

    #[test]
    fn test_security_extractor_generic_fallback() {
        let req = SecurityRequirement {
            scheme_name: "oauth".to_string(),
            scopes: vec![],
            scheme: Some(SecuritySchemeInfo {
                kind: SecuritySchemeKind::OAuth2,
                description: None,
            }),
        };
        let code = security_extractor(&[req]);
        assert_eq!(code, "_auth: web::ReqData<security::Oauth>");
    }
}
