#![deny(missing_docs)]

//! # Function Parsers for Reqwest
//!
//! Parses Rust function items to extract OpenAPI route information from a client
//! using `ra_ap_syntax` AST inspection.

use crate::error::AppResult;
use crate::openapi::parse::ParsedRoute;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasArgList, HasAttrs, HasName};
use ra_ap_syntax::{AstNode, AstToken, SourceFile};
use std::collections::BTreeMap;

/// Extracts OpenAPI routes from a given string of Rust source code for reqwest clients.
///
/// Inspects doc comments (`@OAS_METHOD:` and `@OAS_PATH:`) and falls back to traversing
/// `ast::MethodCallExpr` (`client.get(...)`, `client.post(...)`, etc.) and `ast::MacroCall`
/// (`format!("{}/path", base_url)`) nodes in the function body.
///
/// # Arguments
/// * `code` - The Rust source code to parse.
///
/// # Returns
/// A list of extracted [`ParsedRoute`] definitions.
///
/// # Errors
/// Returns [`crate::error::AppError`] if AST parsing fails critically.
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

    let mut method = None;
    let mut path = None;
    let mut summary = String::new();

    for doc in func.doc_comments() {
        let text = doc.text().trim_start_matches("///").trim();
        if let Some(rest) = text.strip_prefix("@OAS_METHOD:") {
            method = Some(rest.trim().to_uppercase()).filter(|s| !s.is_empty());
        } else if let Some(rest) = text.strip_prefix("@OAS_PATH:") {
            path = Some(rest.trim().to_string()).filter(|s| !s.is_empty());
        } else if !text.is_empty() {
            if !summary.is_empty() {
                summary.push(char::from(10u8));
            }
            summary.push_str(text);
        }
    }

    if let Some(body) = func.body() {
        let (ast_method, ast_path) = extract_method_and_path_from_body(&body);
        method = method.or(ast_method);
        path = path.or(ast_path);
    }

    let (method, path) = (method?, path?);

    let summary_opt = if summary.is_empty() {
        None
    } else {
        Some(summary)
    };

    let route = ParsedRoute {
        path,
        summary: summary_opt.clone(),
        description: summary_opt,
        path_summary: None,
        path_description: None,
        operation_summary: None,
        operation_description: None,
        path_extensions: BTreeMap::new(),
        base_path: None,
        path_servers: None,
        servers_override: None,
        method,
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

/// Inspects `ast::MethodCallExpr` and `ast::MacroCall` nodes inside a function block
/// to extract the HTTP method and path template.
fn extract_method_and_path_from_body(body: &ast::BlockExpr) -> (Option<String>, Option<String>) {
    let mut detected_method = None;
    let mut detected_path = None;

    for node in body.syntax().descendants() {
        if let Some(call) = ast::MethodCallExpr::cast(node.clone()) {
            let name = call
                .name_ref()
                .map(|n| n.text().to_uppercase())
                .unwrap_or_default();
            if matches!(
                name.as_str(),
                "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS" | "TRACE"
            ) {
                detected_method = Some(name);
                if let Some(first_arg) = call.arg_list().and_then(|al| al.args().next()) {
                    if let Some(p) = extract_path_from_expr(&first_arg) {
                        detected_path = Some(p);
                    }
                }
            }
        } else if detected_path.is_none() {
            if let Some(macro_call) = ast::MacroCall::cast(node) {
                if let Some(p) = extract_path_from_macro(&macro_call) {
                    detected_path = Some(p);
                }
            }
        }
    }

    (detected_method, detected_path)
}

fn extract_path_from_expr(expr: &ast::Expr) -> Option<String> {
    for node in expr.syntax().descendants_with_tokens() {
        if let Some(macro_call) = node.as_node().and_then(|n| ast::MacroCall::cast(n.clone())) {
            if let Some(p) = extract_path_from_macro(&macro_call) {
                return Some(p);
            }
        }
        if let Some(str_token) = node.as_token().and_then(|t| ast::String::cast(t.clone())) {
            if let Ok(val) = str_token.value() {
                if let Ok(parsed_url) = url::Url::parse(&val) {
                    return Some(parsed_url.path().to_string());
                }
                if let Some(idx) = val.find('/') {
                    return Some(val[idx..].to_string());
                }
            }
        }
    }
    None
}

fn extract_path_from_macro(macro_call: &ast::MacroCall) -> Option<String> {
    if macro_call.path().is_none_or(|p| p.to_string() != "format") {
        return None;
    }
    let tt = macro_call.token_tree()?;
    for element in tt.syntax().children_with_tokens() {
        if let Some(str_token) = element.into_token().and_then(ast::String::cast) {
            if let Ok(val) = str_token.value() {
                let cleaned = val.trim_start_matches("{}");
                if cleaned.starts_with('/') {
                    return Some(cleaned.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_reqwest_routes_from_docs_and_ast() {
        let code = r#"
        /// Get a user by ID
        ///
        /// Second line
        /// @OAS_METHOD: GET
        /// @OAS_PATH: /users/{id}
        pub async fn get_user() {}

        /// Create a user via AST inspection
        pub async fn create_user(client: &reqwest::Client, base_url: &str) {
            println!("logging");
            let non_slash = format!("hello {}", base_url);
            let url = format!("{}/users", base_url);
            let _ = client.post(&url).send().await;
        }

        pub async fn delete_user(client: &reqwest::Client, base_url: &str, id: i64) {
            let _ = client.delete(format!("{}/users/{id}", base_url)).send().await;
        }

        pub async fn health(client: &reqwest::Client) {
            let _ = client.get("http://localhost:8080/health").send().await;
        }

        /// @OAS_METHOD: PUT
        pub async fn partial_method_doc(client: &reqwest::Client) {
            let _ = client.put("/relative/path").send().await;
        }

        /// @OAS_METHOD: HEAD
        pub async fn partial_method_no_ast_path(client: &reqwest::Client) {
            let _ = client.head("no_slash").send().await;
        }

        /// @OAS_PATH: /doc_path
        pub async fn partial_path_doc(client: &reqwest::Client) {
            let _ = client.patch("no_slash_string").send().await;
        }

        pub async fn no_route_fn(client: &reqwest::Client) {
            let _ = client.get().send();
            let _ = client.post(println!("hi"));
            let _ = client.put("\u{ZZZZ}");
            let _ = format!("\u{ZZZZ}");
            let _ = format!;
        }

        extern "C" {
            fn decl_without_body();
        }
        fn
        "#;
        let routes = extract_routes_from_reqwest_functions(code)
            .expect("Failed to extract routes from reqwest functions");
        assert_eq!(routes.len(), 6);
        assert_eq!(routes[0].method, "GET");
        assert_eq!(routes[0].path, "/users/{id}");
        assert_eq!(
            routes[0].summary.as_deref(),
            Some("Get a user by ID\nSecond line")
        );
        assert_eq!(routes[1].method, "POST");
        assert_eq!(routes[1].path, "/users");
        assert_eq!(
            routes[1].summary.as_deref(),
            Some("Create a user via AST inspection")
        );
        assert_eq!(routes[2].method, "DELETE");
        assert_eq!(routes[2].path, "/users/{id}");
        assert_eq!(routes[3].method, "GET");
        assert_eq!(routes[3].path, "/health");
        assert_eq!(routes[4].method, "PUT");
        assert_eq!(routes[4].path, "/relative/path");
        assert_eq!(routes[5].method, "PATCH");
        assert_eq!(routes[5].path, "/doc_path");
    }
}
