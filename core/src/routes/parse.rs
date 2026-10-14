#![deny(missing_docs)]

//! # Route Parsers
//!
//! Parses route registration logic across Actix Web, Axum, Reqwest, Clap CLI, and MCP
//! by delegating to AST function and method-call parsers.

use crate::error::AppResult;
use crate::openapi::parse::ParsedRoute;

/// Top-level parser that takes source code and extracts Actix Web routes
/// (`#[get("/path")]` attributes and `cfg.route(...)` registrations).
///
/// # Arguments
/// * `code` - The Rust source code to parse.
///
/// # Returns
/// A list of [`ParsedRoute`] definitions.
///
/// # Errors
/// Returns [`crate::error::AppError`] if AST parsing fails.
pub fn parse_actix_routes(code: &str) -> AppResult<Vec<ParsedRoute>> {
    crate::functions::parse::extract_routes_from_functions(code)
}

/// Top-level parser that takes source code and extracts Axum routes
/// (`.route("/path", axum::routing::get(handler))`).
///
/// # Arguments
/// * `code` - The Rust source code to parse.
///
/// # Returns
/// A list of [`ParsedRoute`] definitions.
///
/// # Errors
/// Returns [`crate::error::AppError`] if AST parsing fails.
pub fn parse_axum_routes(code: &str) -> AppResult<Vec<ParsedRoute>> {
    crate::functions::parse::extract_routes_from_axum(code)
}

/// Top-level parser that takes source code and extracts reqwest client routes
/// (`@OAS_METHOD`, `@OAS_PATH`, or `client.get(...)` calls).
///
/// # Arguments
/// * `code` - The Rust source code to parse.
///
/// # Returns
/// A list of [`ParsedRoute`] definitions.
///
/// # Errors
/// Returns [`crate::error::AppError`] if AST parsing fails.
pub fn parse_reqwest_routes(code: &str) -> AppResult<Vec<ParsedRoute>> {
    crate::functions::parse_reqwest::extract_routes_from_reqwest_functions(code)
}

/// Top-level parser that takes source code and extracts Clap CLI command routes.
///
/// # Arguments
/// * `code` - The Rust source code to parse.
///
/// # Returns
/// A list of [`ParsedRoute`] definitions.
///
/// # Errors
/// Returns [`crate::error::AppError`] if AST parsing fails.
pub fn parse_clap_cli_routes(code: &str) -> AppResult<Vec<ParsedRoute>> {
    crate::functions::parse_reqwest::extract_routes_from_reqwest_functions(code)
}

/// Top-level parser that takes source code and extracts MCP tool routes
/// (`@MCP_TOOL`, `@MCP_SERVER_TOOL`, `registry.register_tool`).
///
/// # Arguments
/// * `code` - The Rust source code to parse.
///
/// # Returns
/// A list of [`ParsedRoute`] definitions.
///
/// # Errors
/// Returns [`crate::error::AppError`] if AST parsing fails.
pub fn parse_mcp_routes(code: &str) -> AppResult<Vec<ParsedRoute>> {
    crate::functions::parse::extract_routes_from_mcp(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_all_strategy_routes_delegation() {
        let actix_code = r#"
        #[get("/health")]
        pub async fn health_check() {}
        "#;
        let actix_routes = parse_actix_routes(actix_code).expect("Failed to parse actix routes");
        assert_eq!(actix_routes.len(), 1);
        assert_eq!(actix_routes[0].path, "/health");

        let axum_code = r#"
        pub fn app() -> axum::Router {
            axum::Router::new().route("/items/:id", axum::routing::get(get_item))
        }
        "#;
        let axum_routes = parse_axum_routes(axum_code).expect("Failed to parse axum routes");
        assert_eq!(axum_routes.len(), 1);
        assert_eq!(axum_routes[0].path, "/items/{id}");

        let cli_code = r#"
        /// @OAS_METHOD: POST
        /// @OAS_PATH: /orders
        pub async fn create_order() {}
        "#;
        let cli_routes = parse_clap_cli_routes(cli_code).expect("Failed to parse cli routes");
        assert_eq!(cli_routes.len(), 1);
        assert_eq!(cli_routes[0].path, "/orders");

        let mcp_code = r#"
        /// @MCP_TOOL: get_status
        pub async fn get_status() {}
        "#;
        let mcp_routes = parse_mcp_routes(mcp_code).expect("Failed to parse mcp routes");
        assert_eq!(mcp_routes.len(), 1);
        assert_eq!(mcp_routes[0].path, "/get_status");
    }
}
