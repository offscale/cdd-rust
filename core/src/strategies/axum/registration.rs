#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Route Registration
//!
//! Generates the Axum route registration code.

use crate::openapi::parse::ParsedRoute;

/// Generates the `.route(..., method(...))` segment for an Axum Router.
pub fn route_registration_statement(route: &ParsedRoute, handler_full_path: &str) -> String {
    // Convert OpenAPI path format /path/{param} to Axum format /path/:param
    let mut axum_path = route.path.clone();

    if let Some(bp) = &route.base_path {
        if !axum_path.starts_with(bp) {
            axum_path = format!("{}{}", bp, axum_path);
        }
    }

    // Quick regex replacement simulation for {param} -> :param
    while let Some(start) = axum_path.find('{') {
        if let Some(end) = axum_path[start..].find('}') {
            let actual_end = start + end;
            let param_name = &axum_path[start + 1..actual_end];
            let replacement = format!(":{}", param_name);
            axum_path.replace_range(start..actual_end + 1, &replacement);
        } else {
            break;
        }
    }

    let method = match route.method.as_str() {
        "GET" => "get",
        "POST" => "post",
        "PUT" => "put",
        "DELETE" => "delete",
        "PATCH" => "patch",
        "HEAD" => "head",
        "OPTIONS" => "options",
        "TRACE" => "trace",
        _ => "any", // Fallback for custom verbs or ANY
    };

    if method == "any" {
        format!(
            ".route(\"{}\", axum::routing::any({}))",
            axum_path, handler_full_path
        )
    } else {
        format!(
            ".route(\"{}\", axum::routing::{}({}))",
            axum_path, method, handler_full_path
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::RouteKind;
    use std::collections::BTreeMap;

    #[test]
    fn test_route_registration() {
        let route = ParsedRoute {
            path: "/users/{id}".into(),
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
            handler_name: "get_user".into(),
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

        let statement = route_registration_statement(&route, "handlers::get_user");
        assert_eq!(
            statement,
            ".route(\"/users/:id\", axum::routing::get(handlers::get_user))"
        );
    }
}
