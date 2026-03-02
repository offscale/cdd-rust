#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Function Parsers for Reqwest
//!
//! Parses Rust function items to extract OpenAPI route information from a client.

use crate::error::AppResult;
use crate::openapi::parse::ParsedRoute;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasDocComments, HasName};
use ra_ap_syntax::{AstNode, AstToken, SourceFile};
use std::collections::BTreeMap;

/// Extracts OpenAPI routes from a given string of Rust source code for reqwest clients.
pub fn extract_routes_from_reqwest_functions(code: &str) -> AppResult<Vec<ParsedRoute>> {
    let parse = SourceFile::parse(code, Edition::Edition2021);
    let mut routes = Vec::new();

    for node in parse.tree().syntax().descendants() {
        if let Some(func) = ast::Fn::cast(node) {
            if let Some(route) = parse_reqwest_route(&func) {
                routes.push(route);
            }
        }
    }

    Ok(routes)
}

fn parse_reqwest_route(func: &ast::Fn) -> Option<ParsedRoute> {
    let handler_name = func.name()?.text().to_string();

    let mut method = String::new();
    let mut path = String::new();
    let mut summary = String::new();

    for doc in func.doc_comments() {
        let text = doc.text().trim_start_matches("///").trim();
        if text.starts_with("@OAS_METHOD:") {
            method = text.replace("@OAS_METHOD:", "").trim().to_uppercase();
        } else if text.starts_with("@OAS_PATH:") {
            path = text.replace("@OAS_PATH:", "").trim().to_string();
        } else if !text.is_empty() {
            if !summary.is_empty() {
                summary.push('\n');
            }
            summary.push_str(text);
        }
    }

    if method.is_empty() || path.is_empty() {
        return None;
    }

    let route = ParsedRoute {
        path: path.clone(),
        summary: if summary.is_empty() {
            None
        } else {
            Some(summary.clone())
        },
        description: if summary.is_empty() {
            None
        } else {
            Some(summary)
        },
        path_summary: None,
        path_description: None,
        operation_summary: None,
        operation_description: None,
        path_extensions: BTreeMap::new(),
        base_path: None,
        path_servers: None,
        servers_override: None,
        method: method.clone(),
        handler_name: handler_name.clone(),
        operation_id: Some(handler_name),
        params: vec![],
        path_params: vec![],
        request_body: None,
        raw_request_body: None,
        security: vec![],
        security_defined: false,
        kind: crate::openapi::parse::models::RouteKind::Path,
        tags: vec![],
        response_type: None,
        response_status: Some("200".to_string()),
        response_summary: None,
        response_description: Some("OK".to_string()),
        response_media_type: None,
        response_example: None,
        response_headers: vec![],
        raw_responses: None,
        response_links: None,
        callbacks: vec![],
        deprecated: false,
        external_docs: None,
        extensions: BTreeMap::new(),
    };

    Some(route)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_reqwest_routes() {
        let code = r#"
        /// Get a user by ID
        /// @OAS_METHOD: GET
        /// @OAS_PATH: /users/{id}
        pub async fn get_user() {}
        "#;
        let routes = extract_routes_from_reqwest_functions(code).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].method, "GET");
        assert_eq!(routes[0].path, "/users/{id}");
    }
}
