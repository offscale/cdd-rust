#![deny(missing_docs)]

//! # Handler Builder
//!
//! High-level logic for scaffolding or updating handler files.

use crate::error::AppResult;
use crate::handler_generator::extractors::generate_function;
use crate::handler_generator::parsing::extract_fn_names;
use crate::oas::ParsedRoute;
use crate::strategies::BackendStrategy;

/// Updates or creates a handler module source code.
pub fn update_handler_module(
    source: &str,
    routes: &[ParsedRoute],
    strategy: &impl BackendStrategy,
) -> AppResult<String> {
    let mut new_source = source.to_string();
    let is_new_file = source.trim().is_empty();

    if is_new_file {
        new_source.push_str(&strategy.handler_imports());
        new_source.push('\n');
    }

    let existing_fns = extract_fn_names(&new_source);

    for route in routes {
        if !existing_fns.contains(&route.handler_name) {
            let code = generate_function(route, strategy)?;
            new_source.push_str(&code);
            new_source.push('\n');
        }
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
        let route = ParsedRoute {
            path: "/users".into(),
            method: "GET".into(),
            handler_name: "get_users".into(),
            params: vec![],
            request_body: None,
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        assert!(code.contains("pub async fn get_users() -> impl Responder {"));
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
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module(source, &[route], &strategy).unwrap();
        assert!(code.contains("pub async fn existing_handler"));
        assert!(code.contains("pub async fn new_func"));
    }
}
