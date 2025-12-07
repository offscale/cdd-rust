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

    #[test]
    fn test_custom_registration() {
        let route = ParsedRoute {
            path: "/p".into(),
            base_path: None,
            method: "CUSTOM".into(),
            handler_name: "h".into(),
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
        let stmt = route_registration_statement(&route, "h");
        assert!(stmt.contains("Method::from_bytes(b\"CUSTOM\")"));
    }
}
