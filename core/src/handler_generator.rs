#![deny(missing_docs)]

//! # Handler Generator
//!
//! Generates Actix Web handler functions from parsed OpenAPI routes.
//! This module scaffolds the Rust code required to handle HTTP requests,
//! including resolving path parameters, query strings, and request bodies
//! into strictly typed Actix extractors.

use crate::error::AppResult;
use crate::oas::{ParamSource, ParsedRoute};
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
///
/// # Returns
///
/// * `AppResult<String>` - The updated source code.
pub fn update_handler_module(source: &str, routes: &[ParsedRoute]) -> AppResult<String> {
    let mut new_source = source.to_string();
    let is_new_file = source.trim().is_empty();

    // 1. Initialize Headers if new file
    if is_new_file {
        new_source.push_str("use actix_web::{web, HttpResponse, Responder};\n");
        new_source.push_str("use serde_json::Value;\n");
        new_source.push_str("use uuid::Uuid;\n");
        new_source.push_str("use chrono::{DateTime, Utc, NaiveDate, NaiveDateTime};\n");
        // We assume DTOs might be used. A realistic generator might accept a config for where models live.
        new_source.push('\n');
    }

    // 2. Identify Existing Functions to prevent duplicates
    let existing_fns = extract_fn_names(&new_source);

    // 3. Generate Missing Handlers
    for route in routes {
        if !existing_fns.contains(&route.handler_name) {
            let code = generate_function(route)?;
            new_source.push_str(&code);
            new_source.push('\n');
        }
    }

    Ok(new_source)
}

/// Generates a single async handler function string.
fn generate_function(route: &ParsedRoute) -> AppResult<String> {
    let mut args = Vec::new();

    // 1. Path Parameters
    // We must extract variables from the path string to ensure correct ordering for tuple extraction
    // e.g. "/users/{id}/details/{subId}" -> ["id", "subId"]
    let path_vars = extract_path_vars(&route.path);
    if !path_vars.is_empty() {
        // Map to known types found in oas definition, or default to String
        let types: Vec<String> = path_vars
            .iter()
            .map(|name| {
                find_param_type(route, name, ParamSource::Path)
                    .unwrap_or_else(|| "String".to_string())
            })
            .collect();

        if types.len() == 1 {
            // Single param: id: web::Path<Uuid>
            let var_name = to_snake_case(&path_vars[0]);
            args.push(format!("{}: web::Path<{}>", var_name, types[0]));
        } else {
            // Multiple params: path: web::Path<(Uuid, i32)>
            // Actix expects a tuple for multiple path segments if matching positional
            let type_tuple = types.join(", ");
            args.push(format!("path: web::Path<({})>", type_tuple));
        }
    }

    // 2. Query Parameters
    // As per requirement: "Extracts query parameters into web::Query<Value>"
    let has_query = route.params.iter().any(|p| p.source == ParamSource::Query);
    if has_query {
        args.push("query: web::Query<Value>".to_string());
    }

    // 3. Request Body
    if let Some(body_type) = &route.request_body {
        // As per requirement: "Extracts Request Bodies into web::Json<T>"
        args.push(format!("body: web::Json<{}>", body_type));
    }

    let args_str = args.join(", ");

    // 4. Construct Function Body
    let code = format!(
        "pub async fn {}({}) -> impl Responder {{\n    todo!()\n}}\n",
        route.handler_name, args_str
    );

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
    // Regex to find {param}
    let re = Regex::new(r"\{([^}]+)\}").expect("Invalid regex constant");
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
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            for lc in c.to_lowercase() {
                result.push(lc);
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::RouteParam;

    #[test]
    fn test_scaffold_new_file() {
        let route = ParsedRoute {
            path: "/users".into(),
            method: "GET".into(),
            handler_name: "get_users".into(),
            params: vec![],
            request_body: None,
        };

        let code = update_handler_module("", &[route]).unwrap();
        assert!(code.contains("use actix_web"));
        assert!(code.contains("use serde_json::Value"));
        assert!(code.contains("pub async fn get_users() -> impl Responder {"));
        assert!(code.contains("todo!()"));
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
        };

        let code = update_handler_module(source, &[route]).unwrap();
        assert!(code.contains("pub async fn existing_handler"));
        assert!(code.contains("pub async fn new_func"));
    }

    #[test]
    fn test_skip_duplicate() {
        let source = "pub async fn my_handler() {}";
        let route = ParsedRoute {
            path: "/".into(),
            method: "GET".into(),
            handler_name: "my_handler".into(),
            params: vec![],
            request_body: None,
        };

        let code = update_handler_module(source, &[route]).unwrap();
        // Should assume code didn't change (except trimming/whitespace logic implies append)
        let trimmed = code.trim();
        assert!(trimmed.contains("pub async fn my_handler"));
        // Ensure it appears only once.
        let count = trimmed.matches("pub async fn my_handler").count();
        assert_eq!(count, 1);
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
        };

        let code = update_handler_module("", &[route]).unwrap();
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
        };

        let code = update_handler_module("", &[route]).unwrap();
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
            request_body: Some("SearchFilter".into()),
        };

        let code = update_handler_module("", &[route]).unwrap();
        assert!(code.contains("query: web::Query<Value>"));
        assert!(code.contains("body: web::Json<SearchFilter>"));
    }

    #[test]
    fn test_snake_case_conversion() {
        assert_eq!(to_snake_case("userId"), "user_id");
        assert_eq!(to_snake_case("id"), "id");
        assert_eq!(to_snake_case("camelCaseTemp"), "camel_case_temp");
    }

    #[test]
    fn test_param_finding_fallback() {
        // Define path var 'id' but omit from params array -> Should default to String
        let route = ParsedRoute {
            path: "/unknown/{id}".into(),
            method: "GET".into(),
            handler_name: "unknown".into(),
            params: vec![],
            request_body: None,
        };
        let code = update_handler_module("", &[route]).unwrap();
        assert!(code.contains("id: web::Path<String>"));
    }
}
