#![deny(missing_docs)]

//! # Route Registration
//!
//! Logic for generating the `cfg.service(...)` statements used in Actix's
//! configuration function.

use crate::oas::ParsedRoute;

/// Generates the route registration code statement.
pub fn route_registration_statement(route: &ParsedRoute, handler_full_path: &str) -> String {
    let method = route.method.to_uppercase();
    let actix_method = match method.as_str() {
        "GET" => "get()".to_string(),
        "POST" => "post()".to_string(),
        "PUT" => "put()".to_string(),
        "DELETE" => "delete()".to_string(),
        "PATCH" => "patch()".to_string(),
        "HEAD" => "head()".to_string(),
        "TRACE" => "trace()".to_string(),
        "OPTIONS" => "options()".to_string(),
        "QUERY" => "method(actix_web::http::Method::from_bytes(b\"QUERY\").unwrap())".to_string(),
        str_method => format!(
            "method(actix_web::http::Method::from_bytes(b\"{}\").unwrap())",
            str_method
        ),
    };

    format!(
        "\n    cfg.service(web::resource(\"{}\").route(web::{}.to({})));",
        route.path, actix_method, handler_full_path
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::RouteKind;
    use crate::oas::ParsedRoute;
    use std::collections::BTreeMap;

    #[test]
    fn test_custom_registration() {
        let route = ParsedRoute {
            path: "/p".into(),
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
            method: "CUSTOM".into(),
            handler_name: "h".into(),
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
        let stmt = route_registration_statement(&route, "h");
        assert!(stmt.contains("Method::from_bytes(b\"CUSTOM\")"));
    }
}
