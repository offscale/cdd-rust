#![deny(missing_docs)]

//! # Function Parsers
//!
//! Parses Rust function items and route registrations to extract OpenAPI route information
//! across Actix Web, Axum, Clap CLI, and MCP strategies.

use crate::error::AppResult;
use crate::openapi::parse::models::RouteKind;
use crate::openapi::parse::ParsedRoute;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasArgList, HasAttrs, HasName};
use ra_ap_syntax::{AstNode, SourceFile};
use std::collections::BTreeMap;

/// Extracts OpenAPI routes from a given string of Rust source code.
///
/// Supports:
/// - Actix Web attribute macros (`#[get("/path")]`, `#[post("/path")]`, etc.)
/// - Actix Web `cfg.route("/path", web::get().to(handler))` registrations
/// - Axum `.route("/path", axum::routing::get(handler))` registrations
///
/// # Arguments
/// * `code` - The Rust source code to parse.
///
/// # Returns
/// A list of extracted [`ParsedRoute`] definitions.
///
/// # Errors
/// Returns [`crate::error::AppError`] if AST parsing fails critically.
pub fn extract_routes_from_functions(code: &str) -> AppResult<Vec<ParsedRoute>> {
    let parse = SourceFile::parse(code, Edition::Edition2021);
    let mut routes = Vec::new();

    for node in parse.tree().syntax().descendants() {
        if let Some(func) = ast::Fn::cast(node.clone()) {
            if let Some(route) = parse_actix_route(&func) {
                routes.push(route);
            }
        } else if let Some(call) = ast::MethodCallExpr::cast(node) {
            if let Some(route) = parse_method_call_route(&call) {
                if !routes
                    .iter()
                    .any(|r| r.path == route.path && r.method == route.method)
                {
                    routes.push(route);
                }
            }
        }
    }

    Ok(routes)
}

/// Extracts OpenAPI routes specifically from Axum router `.route("/path", get(handler))`
/// method call expressions or `@OAS_METHOD` / `@OAS_PATH` doc comments.
///
/// # Arguments
/// * `code` - The Rust source code to parse.
///
/// # Returns
/// A list of extracted [`ParsedRoute`] definitions.
///
/// # Errors
/// Returns [`crate::error::AppError`] if AST parsing fails critically.
pub fn extract_routes_from_axum(code: &str) -> AppResult<Vec<ParsedRoute>> {
    let parse = SourceFile::parse(code, Edition::Edition2021);
    let mut routes = Vec::new();

    for node in parse.tree().syntax().descendants() {
        if let Some(call) = ast::MethodCallExpr::cast(node) {
            if let Some(route) = parse_method_call_route(&call) {
                routes.push(route);
            }
        }
    }

    if routes.is_empty() {
        crate::functions::parse_reqwest::extract_routes_from_reqwest_functions(code)
    } else {
        Ok(routes)
    }
}

/// Extracts OpenAPI routes from MCP tool definitions (`/// @MCP_TOOL: name` or `/// @MCP_SERVER_TOOL: name`
/// or `registry.register_tool("name", ...)`).
///
/// # Arguments
/// * `code` - The Rust source code to parse.
///
/// # Returns
/// A list of extracted [`ParsedRoute`] definitions.
///
/// # Errors
/// Returns [`crate::error::AppError`] if AST parsing fails critically.
pub fn extract_routes_from_mcp(code: &str) -> AppResult<Vec<ParsedRoute>> {
    let parse = SourceFile::parse(code, Edition::Edition2021);
    let mut routes = Vec::new();

    for node in parse.tree().syntax().descendants() {
        if let Some(func) = ast::Fn::cast(node.clone()) {
            if let Some(route) = parse_mcp_fn_route(&func) {
                routes.push(route);
            }
        } else if let Some(call) = ast::MethodCallExpr::cast(node) {
            if let Some(route) = parse_mcp_register_call(&call) {
                if !routes.iter().any(|r| r.handler_name == route.handler_name) {
                    routes.push(route);
                }
            }
        }
    }

    Ok(routes)
}

fn build_basic_route(
    path: String,
    method: String,
    handler_name: String,
    summary: Option<String>,
) -> ParsedRoute {
    ParsedRoute {
        path,
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
        method,
        handler_name: handler_name.clone(),
        operation_id: Some(handler_name),
        params: vec![],
        path_params: vec![],
        request_body: None,
        raw_request_body: None,
        security: vec![],
        security_defined: false,
        kind: RouteKind::Path,
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
    }
}

fn parse_actix_route(func: &ast::Fn) -> Option<ParsedRoute> {
    let handler_name = func.name()?.text().to_string();

    for meta in func.attrs().filter_map(|a| a.meta()) {
        let method_str = meta
            .path()
            .map(|p| p.to_string().to_uppercase())
            .unwrap_or_default();

        if matches!(
            method_str.as_str(),
            "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS" | "TRACE"
        ) {
            if let Some((_, tt)) = meta.as_simple_call() {
                let tokens = tt.to_string();
                let route_path = tokens
                    .trim_matches(|c| c == '(' || c == ')' || c == ' ')
                    .trim_matches('"')
                    .to_string();

                let summary = crate::docstrings::parse::extract_doc_comment(func);
                return Some(build_basic_route(
                    route_path,
                    method_str,
                    handler_name,
                    summary,
                ));
            }
        }
    }

    None
}

/// Converts Axum `:param` segments in a route path to OpenAPI `{param}` template format.
fn normalize_axum_path(raw_path: &str) -> String {
    raw_path
        .split('/')
        .map(|segment| {
            if let Some(stripped) = segment.strip_prefix(':') {
                format!("{{{}}}", stripped)
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Parses `.route("/path", ...)` method call expressions from Actix `ServiceConfig` or Axum `Router`.
fn parse_method_call_route(call: &ast::MethodCallExpr) -> Option<ParsedRoute> {
    let name_ref = call.name_ref()?;
    if name_ref.text() != "route" {
        return None;
    }

    let arg_list = call.arg_list()?;
    let mut args = arg_list.args();
    let first_arg = args.next()?;
    let second_arg = args.next()?;

    let raw_path = first_arg.syntax().text().to_string();
    let trimmed_path = raw_path.trim().trim_matches('"');
    if !trimmed_path.starts_with('/') {
        return None;
    }
    let normalized_path = normalize_axum_path(trimmed_path);

    let (method, handler_name) = extract_method_and_handler(&second_arg)?;

    Some(build_basic_route(
        normalized_path,
        method,
        handler_name,
        None,
    ))
}

fn extract_method_and_handler(expr: &ast::Expr) -> Option<(String, String)> {
    let mut detected_method: Option<String> = None;
    let mut handler_arg: Option<ast::Expr> = None;

    for node in expr.syntax().descendants() {
        if let Some(mcall) = ast::MethodCallExpr::cast(node.clone()) {
            handler_arg = mcall.arg_list().and_then(|l| l.args().next());
        } else if let Some(call) = ast::CallExpr::cast(node) {
            let callee = call.expr()?;
            let seg = callee
                .syntax()
                .text()
                .to_string()
                .rsplit("::")
                .next()
                .unwrap_or("")
                .trim()
                .to_uppercase();
            if matches!(
                seg.as_str(),
                "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS" | "TRACE"
            ) {
                detected_method = Some(seg);
            }
            if handler_arg.is_none() {
                handler_arg = call.arg_list().and_then(|l| l.args().next());
            }
        }
    }

    let method = detected_method.unwrap_or_else(|| "POST".to_string());
    let target_expr = handler_arg?;
    let handler_name = target_expr
        .syntax()
        .text()
        .to_string()
        .trim()
        .rsplit("::")
        .next()
        .unwrap_or("")
        .trim()
        .to_string();

    Some((method, handler_name))
}

fn parse_mcp_fn_route(func: &ast::Fn) -> Option<ParsedRoute> {
    let handler_name = func.name()?.text().to_string();
    let mut tool_name = None;
    let mut summary = String::new();

    for doc in func.doc_comments() {
        let text = doc.text().trim_start_matches("///").trim();
        if let Some(rest) = text.strip_prefix("@MCP_TOOL:") {
            tool_name = Some(rest.trim().to_string());
        } else if let Some(rest) = text.strip_prefix("@MCP_SERVER_TOOL:") {
            tool_name = Some(rest.trim().to_string());
        } else if !text.is_empty() {
            if !summary.is_empty() {
                summary.push(char::from(10u8));
            }
            summary.push_str(text);
        }
    }

    let name = tool_name?;
    let summary_opt = if summary.is_empty() {
        None
    } else {
        Some(summary)
    };

    Some(build_basic_route(
        format!("/{}", name),
        "POST".to_string(),
        handler_name,
        summary_opt,
    ))
}

fn parse_mcp_register_call(call: &ast::MethodCallExpr) -> Option<ParsedRoute> {
    let name_ref = call.name_ref()?;
    if name_ref.text() != "register_tool" {
        return None;
    }

    let first_arg = call.arg_list()?.args().next()?;
    let tool_name = first_arg
        .syntax()
        .text()
        .to_string()
        .trim()
        .trim_matches('"')
        .to_string();

    if tool_name.is_empty() {
        return None;
    }

    Some(build_basic_route(
        format!("/{}", tool_name),
        "POST".to_string(),
        tool_name,
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_routes_from_functions() {
        let code = r#"
        /// Get a user by ID
        ///
        /// Second line
        #[]
        #[allow(dead_code)]
        #[get]
        #[get("/users/{id}")]
        pub async fn get_user() {}

        #[post("/users")]
        pub async fn create_user() {}

        pub fn helper() {}
        fn
        "#;

        let routes =
            extract_routes_from_functions(code).expect("Failed to extract routes from functions");
        assert_eq!(routes.len(), 2);

        assert_eq!(routes[0].method, "GET");
        assert_eq!(routes[0].path, "/users/{id}");
        assert_eq!(routes[0].handler_name, "get_user");
        assert_eq!(
            routes[0].summary.as_deref(),
            Some("Get a user by ID\nSecond line")
        );

        assert_eq!(routes[1].method, "POST");
        assert_eq!(routes[1].path, "/users");
        assert_eq!(routes[1].handler_name, "create_user");
    }

    #[test]
    fn test_extract_routes_from_actix_and_axum_registrations() {
        let actix_cfg = r#"
        pub fn config(cfg: &mut web::ServiceConfig) {
            cfg.route("/items/{id}", web::delete().to(handlers::items::delete_item));
            cfg.route("/items/{id}", web::delete().to(handlers::items::delete_item));
            cfg.route("/items", web::put().to(handlers::items::put_item));
            cfg.route("/items", web::patch().to(handlers::items::patch_item));
            cfg.route("/items", web::head().to(handlers::items::head_item));
            cfg.route("/items", web::options().to(handlers::items::options_item));
            cfg.route("/items", web::trace().to(handlers::items::trace_item));
            cfg.route();
            cfg.route("/only_one");
            cfg.route("relative", web::get().to(handlers::items::rel));
            cfg.route("/no_parens", bare_ident);
            cfg.route("/empty_handler", web::get().to());
            cfg.other_call();
            cfg.
        }
        "#;
        let routes = extract_routes_from_functions(actix_cfg).expect("parse actix cfg");
        assert_eq!(routes.len(), 6);
        assert_eq!(routes[0].method, "DELETE");
        assert_eq!(routes[0].path, "/items/{id}");
        assert_eq!(routes[0].handler_name, "delete_item");

        let axum_code = r#"
        pub fn router() -> axum::Router {
            axum::Router::new()
                .route("/users/:id", axum::routing::get(handlers::users::get_user))
                .route("/users", axum::routing::post(handlers::users::create_user))
                .route("/any", axum::routing::any(handlers::users::any_user))
                .layer(trace)
        }
        "#;
        let axum_routes = extract_routes_from_axum(axum_code).expect("parse axum router");
        assert_eq!(axum_routes.len(), 3);
        assert_eq!(
            (
                axum_routes[0].path.as_str(),
                axum_routes[0].method.as_str(),
                axum_routes[0].handler_name.as_str()
            ),
            ("/any", "POST", "any_user")
        );
        assert_eq!(
            (
                axum_routes[1].path.as_str(),
                axum_routes[1].method.as_str(),
                axum_routes[1].handler_name.as_str()
            ),
            ("/users", "POST", "create_user")
        );
        assert_eq!(
            (
                axum_routes[2].path.as_str(),
                axum_routes[2].method.as_str(),
                axum_routes[2].handler_name.as_str()
            ),
            ("/users/{id}", "GET", "get_user")
        );

        // Axum fallback to @OAS_METHOD / @OAS_PATH when no `.route(...)` calls exist
        let doc_annotated = r#"
        /// @OAS_METHOD: GET
        /// @OAS_PATH: /fallback
        pub async fn fallback_handler() {}
        "#;
        let fb_routes = extract_routes_from_axum(doc_annotated).expect("axum fallback");
        assert_eq!(fb_routes.len(), 1);
        assert_eq!(fb_routes[0].path, "/fallback");
    }

    #[test]
    fn test_extract_routes_from_mcp() {
        let mcp_code = r#"
        /// List pets tool
        ///
        /// Extra detail
        /// @MCP_TOOL: list_pets
        pub async fn list_pets() {}

        /// @MCP_SERVER_TOOL: create_pet
        pub async fn create_pet() {}

        pub async fn not_an_mcp_tool() {}

        pub fn register(registry: &mut ToolRegistry) {
            registry.register_tool("create_pet", Box::new(handlers::create_pet));
            registry.register_tool("delete_pet", Box::new(handlers::delete_pet));
            registry.register_tool("", Box::new(handlers::empty));
            registry.register_tool();
            registry.other_method();
            registry.
        }
        fn
        "#;
        let mcp_routes = extract_routes_from_mcp(mcp_code).expect("parse mcp routes");
        assert_eq!(mcp_routes.len(), 3);
        assert_eq!(mcp_routes[0].path, "/list_pets");
        assert_eq!(
            mcp_routes[0].summary.as_deref(),
            Some("List pets tool\nExtra detail")
        );
        assert_eq!(mcp_routes[1].path, "/create_pet");
        assert_eq!(mcp_routes[2].path, "/delete_pet");
    }
}
