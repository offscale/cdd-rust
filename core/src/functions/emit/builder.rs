#![deny(missing_docs)]

//! # Handler Builder
//!
//! High-level logic for scaffolding or updating handler files.

use crate::error::AppResult;
use crate::functions::emit::extractors::{generate_function, generate_query_struct};
use crate::functions::emit::parsing::{extract_fn_names, extract_struct_names, to_snake_case};
use crate::openapi::parse::ParsedRoute;
use crate::strategies::BackendStrategy;
use std::collections::HashSet;

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
    let existing_structs = extract_struct_names(&new_source);
    let mut added_structs = HashSet::new();
    let mut tests_code = String::new();

    for route in routes {
        if !existing_fns.contains(&route.handler_name) {
            if let Some(query_struct) = generate_query_struct(route) {
                if !existing_structs.contains(&query_struct.name)
                    && added_structs.insert(query_struct.name.clone())
                {
                    new_source.push_str(&query_struct.code);
                    new_source.push('\n');

                    tests_code.push_str(&format!(
                        "    #[test]\n    fn test_{}_deserialize() {{\n        let _ = serde_json::from_str::<super::{}>(\"{{}}\");\n    }}\n",
                        to_snake_case(&query_struct.name),
                        query_struct.name
                    ));
                }
            }
            let code = generate_function(route, strategy)?;
            new_source.push_str(&code);
            new_source.push('\n');

            let unit_test = strategy.handler_unit_test(route);
            if !unit_test.is_empty() {
                tests_code.push_str(&unit_test);
                tests_code.push('\n');
            }
        }
    }

    if !tests_code.is_empty() && !new_source.contains("mod tests {") {
        new_source.push_str("\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n");
        new_source.push_str(&tests_code);
        new_source.push_str("}\n");
    }

    Ok(new_source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::RouteKind;
    use crate::openapi::parse::ParsedRoute;
    use crate::strategies::ActixStrategy;
    use std::collections::BTreeMap;

    #[test]
    fn test_scaffold_new_file() {
        let route = ParsedRoute {
            path: "/users".into(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".into(),
            handler_name: "get_users".into(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            tags: vec![],
            extensions: BTreeMap::new(),
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy)
            .expect("Failed to update handler module");
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
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "POST".into(),
            handler_name: "new_func".into(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            tags: vec![],
            extensions: BTreeMap::new(),
        };

        let strategy = ActixStrategy;
        let code = update_handler_module(source, &[route], &strategy)
            .expect("Failed to update handler module");
        assert!(code.contains("pub async fn existing_handler"));
        assert!(code.contains("pub async fn new_func"));
    }
}
