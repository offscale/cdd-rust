#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Route Parsers
//!
//! Parses route registration logic, usually delegating to the function-level parsers
//! to understand the fully composed endpoints.

use crate::error::AppResult;
use crate::openapi::parse::ParsedRoute;

/// Top-level parser that takes source code and extracts all routes
/// by delegating to function AST parsers.
pub fn parse_actix_routes(code: &str) -> AppResult<Vec<ParsedRoute>> {
    crate::functions::parse::extract_routes_from_functions(code)
}

/// Top-level parser that takes source code and extracts all reqwest client routes
/// by delegating to the reqwest function parser.
pub fn parse_reqwest_routes(code: &str) -> AppResult<Vec<ParsedRoute>> {
    crate::functions::parse_reqwest::extract_routes_from_reqwest_functions(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_actix_routes_delegation() {
        let code = r#"
        #[get("/health")]
        pub async fn health_check() {}
        "#;

        let routes = parse_actix_routes(code).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/health");
    }
}
