#![deny(missing_docs)]

//! # Handler Builder
//!
//! High-level logic for scaffolding or updating handler files.

use crate::error::AppResult;
use crate::functions::emit::extractors::{generate_function, generate_query_struct};
use crate::functions::emit::parsing::{extract_fn_names, extract_struct_names, to_snake_case};
use crate::openapi::parse::ParsedRoute;
use crate::strategies::BackendStrategy;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, SourceFile, SyntaxKind};

/// Updates or creates a handler module source code.
///
/// # Arguments
/// * `source` - Input `source` parameter.
/// * `routes` - Input `routes` parameter.
/// * `strategy` - Input `strategy` parameter.
///
/// # Returns
/// Resulting `AppResult<String>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
pub fn update_handler_module(
    source: &str,
    routes: &[ParsedRoute],
    strategy: &impl BackendStrategy,
) -> AppResult<String> {
    update_handler_module_dyn(source, routes, strategy)
}

/// Executes `update_handler_module_dyn` (update handler module dyn).
///
/// # Arguments
/// * `source` - Input `source` parameter.
/// * `routes` - Input `routes` parameter.
/// * `strategy` - Input `strategy` parameter.
///
/// # Returns
/// Resulting `AppResult<String>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn update_handler_module_dyn(
    source: &str,
    routes: &[ParsedRoute],
    strategy: &dyn BackendStrategy,
) -> AppResult<String> {
    let mut new_source = source.to_string();
    let is_new_file = source.trim().is_empty();

    if is_new_file {
        new_source.push_str(&strategy.handler_imports());
        new_source.push('\n');
    }

    let existing_fns = extract_fn_names(&new_source);
    let existing_tests: std::collections::HashSet<String> =
        crate::tests::parse::extract_test_functions(&new_source)?
            .into_iter()
            .collect();
    let mut added_structs = extract_struct_names(&new_source);
    let mut tests_code = String::new();

    for route in routes {
        if !existing_fns.contains(&route.handler_name) {
            if let Some(query_struct) = generate_query_struct(route) {
                if added_structs.insert(query_struct.name.clone()) {
                    new_source.push_str(&query_struct.code);
                    new_source.push('\n');

                    let query_test_name =
                        format!("test_{}_deserialize", to_snake_case(&query_struct.name));
                    if !existing_tests.contains(&query_test_name) {
                        tests_code.push_str(&format!(
                            "    #[test]\n    fn {}() {{\n        let parsed = serde_json::from_str::<super::{}>(\"{{}}\").expect(\"default query deserialization must succeed\");\n        assert!(!format!(\"{{:?}}\", parsed).is_empty());\n    }}\n",
                            query_test_name,
                            query_struct.name
                        ));
                    }
                }
            }
            let code = generate_function(route, strategy).unwrap_or_default();
            new_source.push_str(&code);
            new_source.push('\n');

            let unit_test = strategy.handler_unit_test(route);
            if !unit_test.is_empty() {
                tests_code.push_str(&unit_test);
                tests_code.push('\n');
            }
        }
    }

    if !tests_code.is_empty() {
        inject_into_mod_tests(&mut new_source, &tests_code);
    }

    Ok(new_source)
}

/// Injects generated unit test items into an existing `mod tests` CST node or appends a new `mod tests` block.
///
/// # Arguments
/// * `source` - Input `source` parameter.
/// * `tests_code` - Input `tests_code` parameter.
fn inject_into_mod_tests(source: &mut String, tests_code: &str) {
    let parse = SourceFile::parse(source, Edition::Edition2021);
    let insert_pos = parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Module::cast)
        .find(|m| m.name().is_some_and(|n| n.text() == "tests"))
        .and_then(|m| m.item_list())
        .and_then(|list| {
            list.syntax()
                .last_token()
                .filter(|t| t.kind() == SyntaxKind::R_CURLY)
        })
        .map(|tok| usize::from(tok.text_range().start()));

    if let Some(pos) = insert_pos {
        source.insert_str(pos, tests_code);
    } else {
        source.push_str("\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n");
        source.push_str(tests_code);
        source.push_str("}\n");
    }
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
        let code = update_handler_module(source, std::slice::from_ref(&route), &strategy)
            .expect("Failed to update handler module");
        assert!(code.contains("pub async fn existing_handler"));
        assert!(code.contains("pub async fn new_func"));

        // Re-running on `code` skips `new_func` because it already exists
        let code_again = update_handler_module(&code, std::slice::from_ref(&route), &strategy)
            .expect("idempotent update");
        assert_eq!(code_again, code);

        // Route with query param when Query struct already exists in source
        let mut q_route = route;
        q_route.handler_name = "search_items".into();
        q_route.params = vec![crate::openapi::parse::RouteParam {
            name: "q".into(),
            description: None,
            source: crate::openapi::parse::ParamSource::Query,
            ty: "String".into(),
            content_media_type: None,
            style: None,
            explode: false,
            deprecated: false,
            allow_empty_value: false,
            allow_reserved: false,
            example: None,
            raw_schema: None,
            extensions: BTreeMap::new(),
        }];
        let src_with_struct = "pub struct SearchItemsQuery {}\nmod tests {\n}\n";
        let updated_q =
            update_handler_module(src_with_struct, std::slice::from_ref(&q_route), &strategy)
                .expect("update with existing query struct");
        assert!(updated_q.contains("pub async fn search_items"));

        // Two routes producing the same PascalCase Query struct name ("SearchItemsQuery")
        let mut q_route_collision = q_route.clone();
        q_route_collision.handler_name = "search__items".into();
        let updated_both =
            update_handler_module("", &[q_route.clone(), q_route_collision], &strategy)
                .expect("both");
        assert!(updated_both.contains("pub async fn search_items"));
        assert!(updated_both.contains("pub async fn search__items"));

        // Source where the query test already exists in `mod tests`
        let src_with_existing_test =
            "mod tests {\n    #[test]\n    fn test_search_items_query_deserialize() {}\n}\n";
        let updated_with_test = update_handler_module(
            src_with_existing_test,
            std::slice::from_ref(&q_route),
            &strategy,
        )
        .expect("existing query test");
        assert_eq!(
            updated_with_test
                .matches("fn test_search_items_query_deserialize")
                .count(),
            1
        );

        let reqwest_code =
            update_handler_module("", &[], &crate::strategies::ReqwestStrategy).expect("empty");
        assert!(!reqwest_code.contains("mod tests {"));
    }
}
