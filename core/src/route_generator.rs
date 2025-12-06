#![deny(missing_docs)]

//! # Route Generator
//!
//! Utilities to generate and update Actix Web route configuration files.
//! This module manages the injection of `cfg.service(...)` calls into a centralized
//! configuration function, wiring up generated handlers to specific API paths.
//!
//! It supports adding new routes to an existing configuration file ("Append" mode)
//! by parsing the AST and injecting strictly typed service registration statements
//! via the provided `BackendStrategy`.

use crate::error::AppResult;
use crate::oas::ParsedRoute;
use crate::strategies::BackendStrategy;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, SourceFile, SyntaxKind};

/// Updates the route configuration function to register new services.
///
/// Designed to target a file like `src/http/routes.rs` containing a function signature:
/// `pub fn config(cfg: &mut web::ServiceConfig) { ... }`
///
/// This function uses a statement-based injection approach which is robust against
/// existing code structure (chains vs standalone statements).
///
/// # Arguments
///
/// * `source` - Existing source code of the routes file.
/// * `module_name` - The name of the module where handlers are located (e.g., "users" implies `crate::http::handlers::users`).
/// * `routes` - List of routes to register.
/// * `strategy` - The backend strategy to use for registration syntax.
///
/// # Returns
///
/// * `AppResult<String>` - Updated source code.
pub fn register_routes(
    source: &str,
    module_name: &str,
    routes: &[ParsedRoute],
    strategy: &impl BackendStrategy,
) -> AppResult<String> {
    let mut new_source = source.to_string();
    let is_new_file = source.trim().is_empty();

    // 1. Scaffold if empty
    // Note: The scaffolding logic here currently defaults to Actix imports/signature.
    // Ideally this should also be moved to the strategy trait, but for this refactor step
    // we focus on decoupling the route registration logic.
    if is_new_file {
        new_source.push_str("use actix_web::web;\n");
        // We assume a standard project structure, but user can adjust imports if needed.
        new_source.push_str("use crate::http::handlers;\n\n");
        new_source.push_str("pub fn config(cfg: &mut web::ServiceConfig) {\n}\n");
    }

    // 2. Parse to find existing registrations
    let parse = SourceFile::parse(&new_source, Edition::Edition2021);
    let file = parse.tree();

    let config_fn = file
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)
        .filter(|f| f.name().is_some_and(|n| n.text() == "config"));

    if let Some(func) = config_fn {
        let body = func
            .body()
            .ok_or_else(|| crate::error::AppError::General("Config function has no body".into()))?;

        // We check the raw code of the body to see if a handler is already registered.
        // A robust implementation might parse the method chains, but strict string matching
        // on the handler path is sufficient and safer against formatting variations.
        let existing_code = body.syntax().text().to_string();

        let r_curly = body
            .syntax()
            .last_token()
            .filter(|t| t.kind() == SyntaxKind::R_CURLY)
            .ok_or_else(|| crate::error::AppError::General("Missing } in config".into()))?;

        // We insert before the closing brace.
        let insert_pos: usize = r_curly.text_range().start().into();
        let mut injection = String::new();

        for route in routes {
            let full_handler_path = format!("handlers::{}::{}", module_name, route.handler_name);

            // Check existence (simple string match avoids duplication)
            if !existing_code.contains(&full_handler_path) {
                // Use strategy to generate the statement
                let registration = strategy.route_registration_statement(route, &full_handler_path);
                injection.push_str(&registration);
            }
        }

        if !injection.is_empty() {
            new_source.insert_str(insert_pos, &injection);
        }
    } else {
        return Err(crate::error::AppError::General(
            "Could not find 'config' function to patch".into(),
        ));
    }

    Ok(new_source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::RouteKind;
    use crate::oas::ParsedRoute;
    use crate::strategies::ActixStrategy;

    #[test]
    fn test_scaffold_new_file() {
        let routes = vec![];
        let strategy = ActixStrategy;
        let code = register_routes("", "users", &routes, &strategy).unwrap();
        assert!(code.contains("pub fn config(cfg: &mut web::ServiceConfig)"));
        assert!(code.contains("use crate::http::handlers;"));
        // Should have empty body
        assert!(code.contains("{\n}\n"));
    }

    #[test]
    fn test_register_single_route() {
        let parser_route = ParsedRoute {
            path: "/users".into(),
            method: "GET".into(),
            handler_name: "get_users".into(),
            params: vec![],
            request_body: None,
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
        };

        // Simulating a fresh file scaffolded content
        let source = r#"
use actix_web::web;
use crate::http::handlers;

pub fn config(cfg: &mut web::ServiceConfig) {
}
"#;
        let strategy = ActixStrategy;
        let code = register_routes(source, "users", &[parser_route], &strategy).unwrap();

        // Expect: cfg.service(web::resource("/users").route(web::get().to(handlers::users::get_users)));
        assert!(code.contains("cfg.service(web::resource(\"/users\")"));
        assert!(code.contains(".route(web::get().to(handlers::users::get_users)));"));
    }

    #[test]
    fn test_append_routes_to_existing() {
        let parser_route = ParsedRoute {
            path: "/new".into(),
            method: "POST".into(),
            handler_name: "new_fn".into(),
            params: vec![],
            request_body: None,
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
        };

        // Existing file has one route already with a semicolon
        let source = r#"
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/old").route(web::get().to(handlers::users::old_fn)));
}
"#;
        let strategy = ActixStrategy;
        let code = register_routes(source, "users", &[parser_route], &strategy).unwrap();

        // Check that old code persists
        assert!(code.contains("handlers::users::old_fn"));
        // Check new code added
        assert!(code.contains("handlers::users::new_fn"));
        // Check structural integrity (semicolons present)
        assert_eq!(code.matches(");").count(), 2);
    }

    #[test]
    fn test_skip_existing_route_duplication() {
        let parser_route = ParsedRoute {
            path: "/users".into(),
            method: "POST".into(),
            handler_name: "create_user".into(),
            params: vec![],
            request_body: None,
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
        };

        // Simulation: Route already exists
        let source = r#"
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/users").route(web::post().to(handlers::users::create_user)));
}
"#;
        let strategy = ActixStrategy;
        let code = register_routes(source, "users", &[parser_route], &strategy).unwrap();
        // Should remain identical
        assert_eq!(code, source);
    }

    #[test]
    fn test_append_multiple_statements() {
        let r1 = ParsedRoute {
            path: "/a".into(),
            method: "GET".into(),
            handler_name: "a".into(),
            params: vec![],
            request_body: None,
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
        };
        let r2 = ParsedRoute {
            path: "/b".into(),
            method: "POST".into(),
            handler_name: "b".into(),
            params: vec![],
            request_body: None,
            security: vec![],
            response_type: None,
            kind: RouteKind::Path,
        };

        let source = "pub fn config(cfg: &mut web::ServiceConfig) { }";
        let strategy = ActixStrategy;
        let code = register_routes(source, "mod", &[r1, r2], &strategy).unwrap();

        assert!(code.contains("handlers::mod::a"));
        assert!(code.contains("handlers::mod::b"));
        // Ensure strictly statements
        assert!(code.contains("cfg.service("));
    }

    #[test]
    fn test_missing_config_fn() {
        let strategy = ActixStrategy;
        let res = register_routes("fn other() {}", "mod", &[], &strategy);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(format!("{}", err).contains("Could not find 'config' function"));
    }
}
