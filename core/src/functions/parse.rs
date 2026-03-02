#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Function Parsers
//!
//! Parses Rust function items to extract OpenAPI route information (Actix Web).

use crate::error::AppResult;
use crate::openapi::parse::ParsedRoute;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasAttrs, HasDocComments, HasName};
use ra_ap_syntax::{AstNode, AstToken, SourceFile};
use std::collections::BTreeMap;

/// Extracts OpenAPI routes from a given string of Rust source code.
/// This parses `#[get("/path")]` attributes on functions.
pub fn extract_routes_from_functions(code: &str) -> AppResult<Vec<ParsedRoute>> {
    let parse = SourceFile::parse(code, Edition::Edition2021);
    let mut routes = Vec::new();

    for node in parse.tree().syntax().descendants() {
        if let Some(func) = ast::Fn::cast(node) {
            if let Some(route) = parse_actix_route(&func) {
                routes.push(route);
            }
        }
    }

    Ok(routes)
}

fn parse_actix_route(func: &ast::Fn) -> Option<ParsedRoute> {
    let handler_name = func.name()?.text().to_string();

    for attr in func.attrs() {
        if let Some(meta) = attr.meta() {
            if let Some(path) = meta.path() {
                let method_str = path.to_string().to_uppercase();

                if matches!(
                    method_str.as_str(),
                    "GET" | "POST" | "PUT" | "DELETE" | "PATCH"
                ) {
                    if let Some(tt) = meta.token_tree() {
                        let tokens = tt.to_string();
                        let mut route_path = tokens
                            .trim_matches(|c| c == '(' || c == ')' || c == ' ')
                            .to_string();

                        route_path = route_path.trim_matches('"').to_string();

                        let summary = parse_doc_comments(func);

                        let route = ParsedRoute {
                            path: route_path,
                            summary: summary.clone(),
                            description: summary,
                            path_summary: None,
                            path_description: None,
                            operation_summary: None,
                            operation_description: None,
                            path_extensions: BTreeMap::new(),
                            base_path: None,
                            path_servers: None,
                            servers_override: None,
                            method: method_str,
                            handler_name: handler_name.clone(),
                            operation_id: Some(handler_name.clone()),
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

                        return Some(route);
                    }
                }
            }
        }
    }

    None
}

fn parse_doc_comments(func: &ast::Fn) -> Option<String> {
    let mut docs = Vec::new();
    for doc in func.doc_comments() {
        let text = doc.text().trim_start_matches("///").trim().to_string();
        if !text.is_empty() {
            docs.push(text);
        }
    }

    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_routes_from_functions() {
        let code = r#"
        /// Get a user by ID
        #[get("/users/{id}")]
        pub async fn get_user() {}

        #[post("/users")]
        pub async fn create_user() {}
        "#;

        let routes = extract_routes_from_functions(code).unwrap();
        assert_eq!(routes.len(), 2);

        assert_eq!(routes[0].method, "GET");
        assert_eq!(routes[0].path, "/users/{id}");
        assert_eq!(routes[0].handler_name, "get_user");
        assert_eq!(routes[0].summary.as_deref(), Some("Get a user by ID"));

        assert_eq!(routes[1].method, "POST");
        assert_eq!(routes[1].path, "/users");
        assert_eq!(routes[1].handler_name, "create_user");
    }
}
