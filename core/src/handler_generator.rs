#![deny(missing_docs)]

//! # Handler Generator
//!
//! Generates Actix Web handler functions from parsed OpenAPI routes.
//! This module scaffolds the Rust code required to handle HTTP requests,
//! including resolving path parameters, query strings, headers, cookies, and request bodies
//! into strictly typed extractors.

use crate::error::AppResult;
use crate::oas::{BodyFormat, ParamSource, ParsedRoute};
use crate::strategies::BackendStrategy;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, SourceFile};
use regex::Regex;
use std::collections::HashSet;

/// Updates or creates a handler module source code.
///
/// If `source` is empty, it initializes standard imports required for Actix Web handlers.
/// It parses the existing code to avoid overwriting or duplicating existing handler functions.
///
/// # Arguments
///
/// * `source` - Existing file content (empty string if new file).
/// * `routes` - List of routes to generate handlers for.
/// * `strategy` - The backend strategy (e.g. Actix) used to generate code.
///
/// # Returns
///
/// * `AppResult<String>` - The updated source code.
pub fn update_handler_module(
    source: &str,
    routes: &[ParsedRoute],
    strategy: &impl BackendStrategy,
) -> AppResult<String> {
    let mut new_source = source.to_string();
    let is_new_file = source.trim().is_empty();

    // 1. Initialize Headers if new file
    if is_new_file {
        new_source.push_str(&strategy.handler_imports());
        new_source.push('\n');
    }

    // 2. Identify Existing Functions to prevent duplicates
    let existing_fns = extract_fn_names(&new_source);

    // 3. Generate Missing Handlers
    for route in routes {
        if !existing_fns.contains(&route.handler_name) {
            let code = generate_function(route, strategy)?;
            new_source.push_str(&code);
            new_source.push('\n');
        }
    }

    Ok(new_source)
}

/// Generates a single async handler function string.
fn generate_function(route: &ParsedRoute, strategy: &impl BackendStrategy) -> AppResult<String> {
    let mut args = Vec::new();

    // 1. Path Parameters
    let path_vars = extract_path_vars(&route.path);
    if !path_vars.is_empty() {
        let types: Vec<String> = path_vars
            .iter()
            .map(|name| {
                find_param_type(route, name, ParamSource::Path)
                    .unwrap_or_else(|| "String".to_string())
            })
            .collect();

        let type_signature = strategy.path_extractor(&types);

        if types.len() == 1 {
            let var_name = to_snake_case(&path_vars[0]);
            args.push(format!("{}: {}", var_name, type_signature));
        } else {
            args.push(format!("path: {}", type_signature));
        }
    }

    // 2. Query Parameters
    let has_query = route.params.iter().any(|p| p.source == ParamSource::Query);
    if has_query {
        args.push(format!("query: {}", strategy.query_extractor()));
    }

    // 3. Headers
    for param in route
        .params
        .iter()
        .filter(|p| p.source == ParamSource::Header)
    {
        let var_name = to_snake_case(&param.name);
        let extractor_type = strategy.header_extractor(&param.ty);
        args.push(format!("{}: {}", var_name, extractor_type));
    }

    // 4. Cookies
    for param in route
        .params
        .iter()
        .filter(|p| p.source == ParamSource::Cookie)
    {
        let var_name = to_snake_case(&param.name);
        args.push(format!("{}: {}", var_name, strategy.cookie_extractor()));
    }

    // 5. Request Body
    if let Some(def) = &route.request_body {
        let extractor = match def.format {
            BodyFormat::Json => strategy.body_extractor(&def.ty),
            BodyFormat::Form => strategy.form_extractor(&def.ty),
            BodyFormat::Multipart => strategy.multipart_extractor(),
        };
        args.push(format!("body: {}", extractor));
    }

    // 6. Security Requirements
    let security_arg = strategy.security_extractor(&route.security);
    if !security_arg.is_empty() {
        args.push(security_arg);
    }

    // 7. Construct Function Body
    let code =
        strategy.handler_signature(&route.handler_name, &args, route.response_type.as_deref());

    Ok(code)
}

/// Parses the source using rust-analyzer syntax tree to find all function names.
fn extract_fn_names(source: &str) -> HashSet<String> {
    let parse = SourceFile::parse(source, Edition::Edition2021);
    parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter_map(|f| f.name().map(|n| n.text().to_string()))
        .collect()
}

/// Extracts parameter names from a path template like `/users/{id}`.
fn extract_path_vars(path: &str) -> Vec<String> {
    let re = Regex::new(r"\{([^}]+)}").expect("Invalid regex constant");
    re.captures_iter(path).map(|c| c[1].to_string()).collect()
}

/// Helper to lookup a parameter type from the parsed route definition.
fn find_param_type(route: &ParsedRoute, name: &str, source: ParamSource) -> Option<String> {
    route
        .params
        .iter()
        .find(|p| p.name == name && p.source == source)
        .map(|p| p.ty.clone())
}

/// Converts a string to snake_case for use as a variable name.
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                let prev = chars[i - 1];
                if prev != '-' && prev != '_' && !prev.is_uppercase() {
                    result.push('_');
                }
            }
            for lc in c.to_lowercase() {
                result.push(lc);
            }
        } else {
            result.push(c);
        }
    }
    result.replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::{RouteKind, SecurityRequirement};
    use crate::oas::{BodyFormat, RequestBodyDefinition, RouteParam};
    use crate::strategies::ActixStrategy;

    #[test]
    fn test_scaffold_new_file() {
        let route = ParsedRoute {
            path: "/users".into(),
            method: "GET".into(),
            handler_name: "get_users".into(),
            params: vec![],
            request_body: None,
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        assert!(code.contains("use actix_web"));
        assert!(code.contains("use serde_json::Value"));
        assert!(code.contains("pub async fn get_users() -> impl Responder {"));
        assert!(code.contains("todo!()"));
    }

    #[test]
    fn test_scaffold_with_response_type() {
        let route = ParsedRoute {
            path: "/users".into(),
            method: "GET".into(),
            handler_name: "get_users".into(),
            params: vec![],
            request_body: None,
            security: vec![],
            response_type: Some("Vec<User>".into()),
            kind: RouteKind::Path,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        // Check for specific Result wrapper
        assert!(code.contains("-> actix_web::Result<web::Json<Vec<User>>>"));
    }

    #[test]
    fn test_append_existing() {
        let source = r#"
            use actix_web::*;
            pub async fn existing_handler() {}
        "#;

        let route = ParsedRoute {
            path: "/new".into(),
            method: "POST".into(),
            handler_name: "new_func".into(),
            params: vec![],
            request_body: None,
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module(source, &[route], &strategy).unwrap();
        assert!(code.contains("pub async fn existing_handler"));
        assert!(code.contains("pub async fn new_func"));
    }

    #[test]
    fn test_single_path_param() {
        let route = ParsedRoute {
            path: "/users/{id}".into(),
            method: "GET".into(),
            handler_name: "get_user".into(),
            params: vec![RouteParam {
                name: "id".into(),
                source: ParamSource::Path,
                ty: "Uuid".into(),
            }],
            request_body: None,
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        assert!(code.contains("id: web::Path<Uuid>"));
    }

    #[test]
    fn test_multiple_path_params() {
        let route = ParsedRoute {
            path: "/items/{cat}/{id}".into(),
            method: "GET".into(),
            handler_name: "get_item".into(),
            params: vec![
                RouteParam {
                    name: "cat".into(),
                    source: ParamSource::Path,
                    ty: "String".into(),
                },
                RouteParam {
                    name: "id".into(),
                    source: ParamSource::Path,
                    ty: "i32".into(),
                },
            ],
            request_body: None,
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        // regex finds cat, then id. order relies on regex extraction from string
        assert!(code.contains("path: web::Path<(String, i32)>"));
    }

    #[test]
    fn test_query_and_body() {
        let route = ParsedRoute {
            path: "/search".into(),
            method: "POST".into(),
            handler_name: "search".into(),
            params: vec![RouteParam {
                name: "q".into(),
                source: ParamSource::Query,
                ty: "String".into(),
            }],
            request_body: Some(RequestBodyDefinition {
                ty: "SearchFilter".into(),
                format: BodyFormat::Json,
            }),
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        assert!(code.contains("query: web::Query<Value>"));
        assert!(code.contains("body: web::Json<SearchFilter>"));
    }

    #[test]
    fn test_header_and_cookie_params() {
        let route = ParsedRoute {
            path: "/secure".into(),
            method: "GET".into(),
            handler_name: "secure_route".into(),
            params: vec![
                RouteParam {
                    name: "X-Auth".into(),
                    source: ParamSource::Header,
                    ty: "String".into(),
                },
                RouteParam {
                    name: "Session".into(),
                    source: ParamSource::Cookie,
                    ty: "String".into(),
                },
            ],
            request_body: None,
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        // Expect snake case variable names
        assert!(code.contains("x_auth: web::Header<String>"));
        assert!(code.contains("session: web::Cookie"));
    }

    #[test]
    fn test_security_stub_gen() {
        // Test that security requirement adds an argument
        let route = ParsedRoute {
            path: "/api".into(),
            method: "POST".into(),
            handler_name: "secure_ops".into(),
            params: vec![],
            request_body: None,
            security: vec![SecurityRequirement {
                scheme_name: "ApiKey".into(),
                scopes: vec![],
            }],
            response_type: None,
            kind: RouteKind::Path,
        };
        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        assert!(code.contains("_auth: web::ReqData<ApiKey>"));
    }

    #[test]
    fn test_form_and_multipart() {
        let form_route = ParsedRoute {
            path: "/form".into(),
            method: "POST".into(),
            handler_name: "submit".into(),
            params: vec![],
            request_body: Some(RequestBodyDefinition {
                ty: "SubmitDto".into(),
                format: BodyFormat::Form,
            }),
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[form_route], &strategy).unwrap();
        assert!(code.contains("body: web::Form<SubmitDto>"));

        let multi_route = ParsedRoute {
            path: "/upload".into(),
            method: "POST".into(),
            handler_name: "upload".into(),
            params: vec![],
            request_body: Some(RequestBodyDefinition {
                ty: "Multipart".into(),
                format: BodyFormat::Multipart,
            }),
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
        };
        let code2 = update_handler_module("", &[multi_route], &strategy).unwrap();
        assert!(code2.contains("body: Multipart"));
    }

    #[test]
    fn test_snake_case_conversion() {
        assert_eq!(to_snake_case("userId"), "user_id");
        assert_eq!(to_snake_case("id"), "id");
        assert_eq!(to_snake_case("camelCaseTemp"), "camel_case_temp");
        assert_eq!(to_snake_case("X-Forwarded-For"), "x_forwarded_for");
    }
}
