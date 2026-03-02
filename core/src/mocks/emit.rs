#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Mock Generation
//!
//! Emits mock objects based on the OpenAPI definitions or IR models.

use crate::classes::parse::models::ParsedStruct;
use crate::error::AppResult;
use crate::openapi::parse::ParsedRoute;

/// Generates mock handlers or test objects from parsed components.
pub fn generate_mock_structs(structs: &[ParsedStruct]) -> AppResult<String> {
    let mut out = String::new();
    for s in structs {
        out.push_str(&format!("pub struct Mock{} {{}}\n", s.name));
    }
    Ok(out)
}

/// Generates mock handlers for routes.
pub fn generate_mock_routes(routes: &[ParsedRoute]) -> AppResult<String> {
    let mut out = String::new();
    for r in routes {
        out.push_str(&format!("pub async fn mock_{}() {{}}\n", r.handler_name));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::RouteKind;
    use std::collections::BTreeMap;

    #[test]
    fn test_generate_mock_routes() {
        let route = ParsedRoute {
            path: "/ping".to_string(),
            summary: None,
            description: None,
            path_summary: None,
            path_description: None,
            operation_summary: None,
            operation_description: None,
            path_extensions: BTreeMap::new(),
            base_path: None,
            path_servers: None,
            servers_override: None,
            method: "GET".to_string(),
            handler_name: "ping".to_string(),
            operation_id: None,
            params: vec![],
            path_params: vec![],
            request_body: None,
            raw_request_body: None,
            tags: vec![],
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            raw_responses: None,
            response_links: None,
            callbacks: vec![],
            kind: RouteKind::Path,
            extensions: BTreeMap::new(),
            external_docs: None,
            deprecated: false,
        };
        let code = generate_mock_routes(&[route]).unwrap();
        assert!(code.contains("pub async fn mock_ping() {}"));
    }
}
