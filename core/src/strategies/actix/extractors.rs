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

/// Generates the type string for strongly typed query extraction.
pub fn typed_query_extractor(inner_type: &str) -> String {
    format!("web::Query<{}>", inner_type)
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

/// Generates the type string for Text body extraction.
pub fn text_extractor(_body_type: &str) -> String {
    "String".to_string()
}

/// Generates the type string for Binary body extraction.
pub fn bytes_extractor(_body_type: &str) -> String {
    "web::Bytes".to_string()
}

/// Generates the type string for Security extraction (Guard/ReqData).
pub fn security_extractor(requirements: &[SecurityRequirement]) -> String {
    if requirements.is_empty() {
        return "".to_string();
    }

    let req = &requirements[0];
    let base_name = to_snake_case(&req.scheme_name);

    // Use scheme info if available to generate strict types
    if let Some(info) = &req.scheme {
        match &info.kind {
            // 1. HTTP Schemes (Basic/Bearer) - Native Extractors available
            SecuritySchemeKind::Http { scheme, .. } => match scheme.to_lowercase().as_str() {
                "bearer" => format!(
                    "{}_auth: actix_web_httpauth::extractors::bearer::BearerAuth",
                    base_name
                ),
                "basic" => format!(
                    "{}_auth: actix_web_httpauth::extractors::basic::BasicAuth",
                    base_name
                ),
                _ => generate_typed_req_data(req, "Authenticated"),
            },

            // 2. API Keys
            SecuritySchemeKind::ApiKey { in_loc, .. } => match in_loc {
                ParamSource::Query => {
                    // ApiKey in Query is just a query param.
                    // We extract as generic Value to allow code to check presence.
                    format!("{}_key: web::Query<serde_json::Value>", base_name)
                }
                // Header/Cookie keys usually handled by Middleware, injected as ReqData
                ParamSource::Header | ParamSource::Cookie => generate_typed_req_data(req, "ApiKey"),
                _ => generate_typed_req_data(req, "ApiKey"),
            },

            // 3. Complex Flows (OAuth2)
            SecuritySchemeKind::OAuth2 => generate_typed_req_data(req, "OAuth2"),

            // 4. Complex Flows (OIDC)
            SecuritySchemeKind::OpenIdConnect => generate_typed_req_data(req, "Oidc"),

            // Other schemes
            _ => generate_typed_req_data(req, "Authenticated"),
        }
    } else {
        // Fallback for when components are missing or unresolvable.
        // We use the scheme_name as the Type name, assuming the user has defined a corresponding
        // struct in `crate::security` to handle this scheme.
        let type_name = to_pascal_case(&req.scheme_name);
        generate_typed_req_data(req, &type_name)
    }
}

/// Helper to generate `web::ReqData<security::TYPE<SCOPES>>`
fn generate_typed_req_data(req: &SecurityRequirement, type_name: &str) -> String {
    let var_name = if type_name == "Authenticated" {
        "_auth".to_string()
    } else {
        match type_name {
            "OAuth2" => "oauth".to_string(),
            "Oidc" => "oidc".to_string(),
            "ApiKey" => "api_key".to_string(),
            other => to_snake_case(other),
        }
    };

    // Scopes are critical for OAuth/OIDC contract enforcement in the handler signature.
    // We generate a Tuple of scope types if multiple are present.
    if req.scopes.is_empty() {
        format!("{}: web::ReqData<security::{}>", var_name, type_name)
    } else {
        let normalized_scopes: Vec<String> = req
            .scopes
            .iter()
            .map(|s| format!("security::scopes::{}", to_pascal_case(s)))
            .collect();

        let scopes_param = if normalized_scopes.len() == 1 {
            normalized_scopes[0].clone()
        } else {
            format!("({})", normalized_scopes.join(", "))
        };

        format!(
            "{}: web::ReqData<security::{}<{}>>",
            var_name, type_name, scopes_param
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
        assert_eq!(
            typed_query_extractor("SearchQuery"),
            "web::Query<SearchQuery>"
        );
        assert_eq!(body_extractor("Dto"), "web::Json<Dto>");
        assert_eq!(text_extractor("String"), "String");
        assert_eq!(bytes_extractor("Vec<u8>"), "web::Bytes");
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
    fn test_security_extractor_oauth2_with_scopes() {
        let req = SecurityRequirement {
            scheme_name: "oauth".to_string(),
            scopes: vec!["read:users".into(), "write:users".into()],
            scheme: Some(SecuritySchemeInfo {
                kind: SecuritySchemeKind::OAuth2,
                description: None,
            }),
        };
        let code = security_extractor(&[req]);

        // Expect specific OAuth2 type wrapping the scopes tuple
        assert_eq!(
            code,
            "oauth: web::ReqData<security::OAuth2<(security::scopes::ReadUsers, security::scopes::WriteUsers)>>"
        );
    }

    #[test]
    fn test_security_extractor_oidc() {
        let req = SecurityRequirement {
            scheme_name: "sso".to_string(),
            scopes: vec![],
            scheme: Some(SecuritySchemeInfo {
                kind: SecuritySchemeKind::OpenIdConnect,
                description: None,
            }),
        };
        let code = security_extractor(&[req]);

        assert_eq!(code, "oidc: web::ReqData<security::Oidc>");
    }

    #[test]
    fn test_security_extractor_api_key_header() {
        let req = SecurityRequirement {
            scheme_name: "ApiKey".to_string(),
            scopes: vec![],
            scheme: Some(SecuritySchemeInfo {
                kind: SecuritySchemeKind::ApiKey {
                    name: "X-API-KEY".into(),
                    in_loc: ParamSource::Header,
                },
                description: None,
            }),
        };
        let code = security_extractor(&[req]);
        assert_eq!(code, "api_key: web::ReqData<security::ApiKey>");
    }
}
