#![deny(missing_docs)]

//! # Strategy Trait Hierarchy
//!
//! Defines the domain-segregated code generation traits:
//! - [`CodegenStrategy`]: Shared target metadata and module generation hooks.
//! - [`HttpServerStrategy`]: HTTP server extractors, handler signatures, route registration, and server test harnesses (`ActixStrategy`, `AxumStrategy`).
//! - [`HttpClientStrategy`]: HTTP client method signatures and mock-server client tests (`ReqwestStrategy`).
//! - [`CliStrategy`]: `clap::Subcommand` enum generation and CLI execution dispatch (`ClapCliStrategy`).
//! - [`McpStrategy`]: JSON-RPC / SSE tool registration and schema dispatch (`McpClientStrategy`, `McpServerStrategy`).
//! - [`BackendStrategy`]: Unified adapter trait used by shared AST emitters (`functions::emit`, `routes::emit`, `tests::emit`).

use crate::openapi::parse::models::{ContentMediaType, SecurityRequirementGroup};
use crate::openapi::parse::ParsedRoute;
use crate::openapi::parse::RequestBodyDefinition;

/// Strongly typed category of code generation strategy, replacing boolean type-checking
/// methods (`is_server()`, `is_cli()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyKind {
    /// HTTP Web Server (`ActixStrategy`, `AxumStrategy`).
    HttpServer,
    /// HTTP Client SDK (`ReqwestStrategy`).
    HttpClient,
    /// Command-Line Interface (`ClapCliStrategy`).
    Cli,
    /// Model Context Protocol Client or Server (`McpClientStrategy`, `McpServerStrategy`).
    Mcp,
}

/// Core code generation trait providing shared target metadata and module generation hooks.
pub trait CodegenStrategy {
    /// Returns the [`StrategyKind`] for this strategy.
    ///
    /// # Returns
    /// The [`StrategyKind`] describing the target domain.
    fn kind(&self) -> StrategyKind;

    /// Returns the standard imports for a generated module in this strategy.
    ///
    /// # Returns
    /// Rust `use` statements as a [`String`].
    fn module_imports(&self) -> String;

    /// Generates the primary function or struct+function definition for `route`.
    ///
    /// # Arguments
    /// * `route` - The parsed OpenAPI route definition.
    /// * `args` - Extracted parameter declarations (`name: Type`).
    ///
    /// # Returns
    /// Generated Rust function/struct code.
    fn emit_handler(&self, route: &ParsedRoute, args: &[String]) -> String;

    /// Returns extra Cargo.toml dependency lines required by this strategy's generated package.
    ///
    /// # Returns
    /// Additional TOML dependency lines (empty by default).
    fn extra_cargo_dependencies(&self) -> String {
        String::new()
    }
}

/// Strategy trait for HTTP web server frameworks (`ActixStrategy`, `AxumStrategy`).
pub trait HttpServerStrategy: CodegenStrategy {
    /// Generates the type string for path parameter extraction.
    ///
    /// # Arguments
    /// * `inner_types` - The Rust types of the path parameters.
    fn server_path_extractor(&self, inner_types: &[String]) -> String;

    /// Generates the type string for untyped query parameter extraction.
    fn server_query_extractor(&self) -> String;

    /// Generates the type string for strongly typed query struct extraction.
    ///
    /// # Arguments
    /// * `inner_type` - The Rust struct type representing query parameters.
    fn server_typed_query_extractor(&self, inner_type: &str) -> String;

    /// Generates the type string for OAS 3.2 `querystring` extraction.
    ///
    /// # Arguments
    /// * `inner_type` - The Rust type the entire query string maps to.
    /// * `content_media_type` - Optional media type of the query string.
    fn server_query_string_extractor(
        &self,
        inner_type: &str,
        content_media_type: Option<&ContentMediaType>,
    ) -> String;

    /// Generates the type string for header parameter extraction.
    ///
    /// # Arguments
    /// * `inner_type` - The Rust type of the header parameter.
    fn server_header_extractor(&self, inner_type: &str) -> String;

    /// Generates the type string for cookie parameter extraction.
    fn server_cookie_extractor(&self) -> String;

    /// Generates the type string for JSON request body extraction.
    ///
    /// # Arguments
    /// * `body_type` - The Rust type of the JSON body.
    fn server_body_extractor(&self, body_type: &str) -> String;

    /// Generates the type string for URL-encoded form body extraction.
    ///
    /// # Arguments
    /// * `body_type` - The Rust type of the form body.
    fn server_form_extractor(&self, body_type: &str) -> String;

    /// Generates the type string for multipart body extraction.
    ///
    /// # Arguments
    /// * `body_type` - The Rust type of the multipart body.
    fn server_multipart_extractor(&self, body_type: &str) -> String;

    /// Generates the type string for plain-text body extraction.
    ///
    /// # Arguments
    /// * `body_type` - The Rust type of the text body.
    fn server_text_extractor(&self, body_type: &str) -> String;

    /// Generates the type string for binary body extraction.
    ///
    /// # Arguments
    /// * `body_type` - The Rust type of the binary body.
    fn server_bytes_extractor(&self, body_type: &str) -> String;

    /// Generates the type string for security guard/principal extraction.
    ///
    /// # Arguments
    /// * `requirements` - Security requirement groups for the operation.
    fn server_security_extractor(&self, requirements: &[SecurityRequirementGroup]) -> String;

    /// Generates the service route registration statement.
    ///
    /// # Arguments
    /// * `route` - The parsed OpenAPI route definition.
    /// * `handler_full_path` - Qualified path to the handler function.
    fn server_route_registration(&self, route: &ParsedRoute, handler_full_path: &str) -> String;
}

/// Strategy trait for HTTP client SDK generators (`ReqwestStrategy`).
pub trait HttpClientStrategy: CodegenStrategy {
    /// Generates OpenAPI doc-comment annotations (`@OAS_METHOD`, `@OAS_PATH`) for a client function.
    ///
    /// # Arguments
    /// * `route` - The parsed OpenAPI route definition.
    fn client_operation_docs(&self, route: &ParsedRoute) -> String;

    /// Generates a mock-server integration test for the client method.
    ///
    /// # Arguments
    /// * `route` - The parsed OpenAPI route definition.
    /// * `crate_name` - Crate/module identifier for the generated client.
    fn client_mock_test(&self, route: &ParsedRoute, crate_name: &str) -> String;
}

/// Strategy trait for CLI binary generators (`ClapCliStrategy`).
pub trait CliStrategy: CodegenStrategy {
    /// Generates the complete `src/main.rs` Clap CLI entrypoint dispatching all `module_names`.
    ///
    /// # Arguments
    /// * `module_subdir` - Subdirectory name under `src/` (e.g. `"handlers"`).
    /// * `module_names` - List of generated command module names.
    fn generate_cli_main(&self, module_subdir: &str, module_names: &[String]) -> String;

    /// Generates a CLI argument parsing and execution test for `route`.
    ///
    /// # Arguments
    /// * `route` - The parsed OpenAPI route definition.
    /// * `crate_name` - Crate/module identifier for the generated CLI package.
    fn cli_command_test(&self, route: &ParsedRoute, crate_name: &str) -> String;
}

/// Strategy trait for Model Context Protocol generators (`McpClientStrategy`, `McpServerStrategy`).
pub trait McpStrategy: CodegenStrategy {
    /// Generates the MCP tool registration statement for `route`.
    ///
    /// # Arguments
    /// * `route` - The parsed OpenAPI route definition.
    /// * `handler_full_path` - Qualified path to the generated tool handler.
    fn mcp_tool_registration(&self, route: &ParsedRoute, handler_full_path: &str) -> String;
}

/// Strategy trait for Arazzo 1.1.0 workflow model, orchestrator, and E2E test generation.
pub trait WorkflowStrategy {
    /// Emits strongly typed Rust structs for workflow inputs, step outputs, and workflow outputs.
    ///
    /// # Arguments
    /// * `doc` - The parsed [`crate::arazzo::models::ArazzoDocument`].
    /// * `linked_specs` - Map of linked [`crate::openapi::parse::document::ParsedOpenApi`] specifications.
    ///
    /// # Returns
    /// Generated Rust model definitions.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError`] if emission fails.
    fn emit_workflow_models(
        &self,
        doc: &crate::arazzo::models::ArazzoDocument,
        linked_specs: &std::collections::BTreeMap<
            String,
            crate::openapi::parse::document::ParsedOpenApi,
        >,
    ) -> crate::error::AppResult<String>;

    /// Emits the workflow execution module (Client SDK `WorkflowsClient`, Clap CLI subcommands reusing SDK, or Server workflow handlers).
    ///
    /// # Arguments
    /// * `doc` - The parsed [`crate::arazzo::models::ArazzoDocument`].
    /// * `linked_specs` - Map of linked [`crate::openapi::parse::document::ParsedOpenApi`] specifications.
    ///
    /// # Returns
    /// Generated Rust workflow module source code.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError`] if emission fails.
    fn emit_workflow_module(
        &self,
        doc: &crate::arazzo::models::ArazzoDocument,
        linked_specs: &std::collections::BTreeMap<
            String,
            crate::openapi::parse::document::ParsedOpenApi,
        >,
    ) -> crate::error::AppResult<String>;

    /// Emits end-to-end integration tests for all workflows in `doc`.
    ///
    /// # Arguments
    /// * `doc` - The parsed [`crate::arazzo::models::ArazzoDocument`].
    /// * `crate_name` - Crate/module identifier for the generated package.
    ///
    /// # Returns
    /// Generated Rust test module source code.
    ///
    /// # Errors
    /// Returns [`crate::error::AppError`] if emission fails.
    fn emit_workflow_tests(
        &self,
        doc: &crate::arazzo::models::ArazzoDocument,
        crate_name: &str,
    ) -> crate::error::AppResult<String>;
}

/// Unified strategy interface used by the shared AST scaffolding and test emitters.
///
/// Non-server strategies inherit sensible defaults for server-only extractors and route
/// registration hooks so they only need to implement methods relevant to their target.
pub trait BackendStrategy: CodegenStrategy {
    /// Returns the standard imports for a handler file in this framework.
    fn handler_imports(&self) -> String {
        self.module_imports()
    }

    /// Generates a handler function signature.
    fn handler_signature(&self, route: &ParsedRoute, args: &[String]) -> String {
        self.emit_handler(route, args)
    }

    /// Optional extra doc comments to insert above the handler.
    fn extra_handler_docs(&self, _route: &ParsedRoute) -> String {
        String::new()
    }

    /// Generates the type string for path parameter extraction.
    fn path_extractor(&self, inner_types: &[String]) -> String {
        if inner_types.len() == 1 {
            inner_types[0].clone()
        } else {
            format!("({})", inner_types.join(", "))
        }
    }

    /// Generates the type string for query parameter extraction.
    fn query_extractor(&self) -> String {
        "serde_json::Value".to_string()
    }

    /// Generates the type string for strongly typed query parameter extraction.
    fn typed_query_extractor(&self, inner_type: &str) -> String {
        inner_type.to_string()
    }

    /// Generates the type string for the entire Query String extraction (OAS 3.2).
    fn query_string_extractor(
        &self,
        inner_type: &str,
        _content_media_type: Option<&ContentMediaType>,
    ) -> String {
        inner_type.to_string()
    }

    /// Generates the type string for Header parameter extraction.
    fn header_extractor(&self, inner_type: &str) -> String {
        inner_type.to_string()
    }

    /// Generates the type string for Cookie parameter extraction.
    fn cookie_extractor(&self) -> String {
        "String".to_string()
    }

    /// Generates the type string for JSON request body extraction.
    fn body_extractor(&self, body_type: &str) -> String {
        body_type.to_string()
    }

    /// Generates the type string for Form request body extraction.
    fn form_extractor(&self, body_type: &str) -> String {
        body_type.to_string()
    }

    /// Generates the type string for Multipart request body extraction.
    fn multipart_extractor(&self, body_type: &str) -> String {
        body_type.to_string()
    }

    /// Generates the type string for Text request body extraction.
    fn text_extractor(&self, body_type: &str) -> String {
        body_type.to_string()
    }

    /// Generates the type string for Binary request body extraction.
    fn bytes_extractor(&self, body_type: &str) -> String {
        body_type.to_string()
    }

    /// Generates the parameter declaration string for a Security extraction/Guard.
    fn security_extractor(&self, _requirements: &[SecurityRequirementGroup]) -> String {
        String::new()
    }

    /// Generates the initial route registration file skeleton when creating a new file.
    fn route_registration_skeleton(&self) -> String {
        "use actix_web::web;\nuse crate::http::handlers;\n\npub fn config(cfg: &mut web::ServiceConfig) {\n}\n".to_string()
    }

    /// Generates the route registration code statement (empty for client/CLI targets).
    fn route_registration_statement(
        &self,
        _route: &ParsedRoute,
        _handler_full_path: &str,
    ) -> String {
        String::new()
    }

    // --- Test Generation ---

    /// Returns the standard imports for a test file in this framework.
    fn test_imports(&self) -> String;

    /// Returns the test function signature (including attributes).
    fn test_fn_signature(&self, fn_name: &str) -> String;

    /// Returns code to initialize the application for testing.
    fn test_app_init(&self, app_factory: &str) -> String;

    /// Returns the code snippet that attaches a dummy body to the request.
    fn test_body_setup_code(&self, body: &RequestBodyDefinition) -> String;

    /// Returns code to build the request object.
    fn test_request_builder(&self, method: &str, uri: &str, body_setup: &str) -> String;

    /// Returns code to execute the request against the app.
    fn test_api_call(&self) -> String;

    /// Returns assertion code for the response.
    fn test_assertion(&self) -> String;

    /// Returns the helper function code for validating responses against OpenAPI.
    fn test_validation_helper(&self) -> String {
        String::new()
    }

    /// Generates a unit test for a specific handler function to be placed alongside it.
    fn handler_unit_test(&self, _route: &ParsedRoute) -> String {
        String::new()
    }

    /// Allows the strategy to fully override test function generation.
    fn generate_custom_test(&self, _route: &ParsedRoute, _app_factory: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::RouteKind;
    use crate::strategies::{
        ClapCliStrategy, McpClientStrategy, McpServerStrategy, ReqwestStrategy,
    };
    use std::collections::BTreeMap;

    struct DefaultDummyStrategy;

    impl CodegenStrategy for DefaultDummyStrategy {
        fn kind(&self) -> StrategyKind {
            StrategyKind::HttpClient
        }

        fn module_imports(&self) -> String {
            "use std::fmt;\n".to_string()
        }

        fn emit_handler(&self, route: &ParsedRoute, _args: &[String]) -> String {
            format!("pub fn {}() {{}}\n", route.handler_name)
        }
    }

    impl BackendStrategy for DefaultDummyStrategy {
        fn test_imports(&self) -> String {
            "use super::*;\n".to_string()
        }

        fn test_fn_signature(&self, fn_name: &str) -> String {
            format!("fn test_{}() {{\n", fn_name)
        }

        fn test_app_init(&self, _app_factory: &str) -> String {
            "let _ = 1;\n".to_string()
        }

        fn test_body_setup_code(&self, _body: &RequestBodyDefinition) -> String {
            "let _ = 2;\n".to_string()
        }

        fn test_request_builder(&self, _method: &str, _uri: &str, _body_setup: &str) -> String {
            "let _ = 3;\n".to_string()
        }

        fn test_api_call(&self) -> String {
            "let _ = 4;\n".to_string()
        }

        fn test_assertion(&self) -> String {
            "assert!(true);\n".to_string()
        }
    }

    fn sample_route() -> ParsedRoute {
        ParsedRoute {
            path: "/pets".into(),
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
            handler_name: "list_pets".into(),
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
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        }
    }

    #[test]
    fn test_default_strategy_trait_methods_and_delegation() {
        let route = sample_route();
        let body = RequestBodyDefinition {
            ty: "Pet".to_string(),
            description: None,
            media_type: "application/json".to_string(),
            format: crate::openapi::parse::BodyFormat::Json,
            required: true,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
            example: None,
        };

        let dummy: &dyn BackendStrategy = &DefaultDummyStrategy;
        assert_eq!(dummy.kind(), StrategyKind::HttpClient);
        assert_eq!(dummy.module_imports(), "use std::fmt;\n");
        assert_eq!(dummy.emit_handler(&route, &[]), "pub fn list_pets() {}\n");
        assert_eq!(dummy.handler_imports(), "use std::fmt;\n");
        assert_eq!(
            dummy.handler_signature(&route, &[]),
            "pub fn list_pets() {}\n"
        );
        assert!(dummy.extra_cargo_dependencies().is_empty());
        assert!(dummy.extra_handler_docs(&route).is_empty());
        assert_eq!(dummy.path_extractor(&["i32".to_string()]), "i32");
        assert_eq!(
            dummy.path_extractor(&["i32".to_string(), "String".to_string()]),
            "(i32, String)"
        );
        assert_eq!(dummy.query_extractor(), "serde_json::Value");
        assert_eq!(dummy.typed_query_extractor("Q"), "Q");
        assert_eq!(dummy.query_string_extractor("Q", None), "Q");
        assert_eq!(dummy.header_extractor("H"), "H");
        assert_eq!(dummy.cookie_extractor(), "String");
        assert_eq!(dummy.body_extractor("B"), "B");
        assert_eq!(dummy.form_extractor("F"), "F");
        assert_eq!(dummy.multipart_extractor("M"), "M");
        assert_eq!(dummy.text_extractor("T"), "T");
        assert_eq!(dummy.bytes_extractor("Vec<u8>"), "Vec<u8>");
        assert_eq!(dummy.security_extractor(&[]), "");
        assert!(dummy.route_registration_statement(&route, "h").is_empty());
        assert_eq!(dummy.test_imports(), "use super::*;\n");
        assert_eq!(dummy.test_fn_signature("pets"), "fn test_pets() {\n");
        assert_eq!(dummy.test_app_init("app"), "let _ = 1;\n");
        assert_eq!(dummy.test_body_setup_code(&body), "let _ = 2;\n");
        assert_eq!(dummy.test_request_builder("GET", "/", ""), "let _ = 3;\n");
        assert_eq!(dummy.test_api_call(), "let _ = 4;\n");
        assert_eq!(dummy.test_assertion(), "assert!(true);\n");
        assert!(dummy.test_validation_helper().is_empty());
        assert!(dummy.handler_unit_test(&route).is_empty());
        assert!(dummy.generate_custom_test(&route, "app").is_none());

        let direct = DefaultDummyStrategy;
        assert_eq!(CodegenStrategy::kind(&direct), StrategyKind::HttpClient);
        assert_eq!(BackendStrategy::handler_imports(&direct), "use std::fmt;\n");
        assert_eq!(
            BackendStrategy::handler_signature(&direct, &route, &[]),
            "pub fn list_pets() {}\n"
        );
        assert!(CodegenStrategy::extra_cargo_dependencies(&direct).is_empty());
        assert!(BackendStrategy::extra_handler_docs(&direct, &route).is_empty());
        assert_eq!(
            BackendStrategy::path_extractor(&direct, &["i32".to_string()]),
            "i32"
        );
        assert_eq!(
            BackendStrategy::path_extractor(&direct, &["i32".to_string(), "String".to_string()]),
            "(i32, String)"
        );
        assert_eq!(
            BackendStrategy::query_extractor(&direct),
            "serde_json::Value"
        );
        assert_eq!(BackendStrategy::typed_query_extractor(&direct, "Q"), "Q");
        assert_eq!(
            BackendStrategy::query_string_extractor(&direct, "Q", None),
            "Q"
        );
        assert_eq!(BackendStrategy::header_extractor(&direct, "H"), "H");
        assert_eq!(BackendStrategy::cookie_extractor(&direct), "String");
        assert_eq!(BackendStrategy::body_extractor(&direct, "B"), "B");
        assert_eq!(BackendStrategy::form_extractor(&direct, "F"), "F");
        assert_eq!(BackendStrategy::multipart_extractor(&direct, "M"), "M");
        assert_eq!(BackendStrategy::text_extractor(&direct, "T"), "T");
        assert_eq!(
            BackendStrategy::bytes_extractor(&direct, "Vec<u8>"),
            "Vec<u8>"
        );
        assert_eq!(BackendStrategy::security_extractor(&direct, &[]), "");
        assert!(BackendStrategy::route_registration_statement(&direct, &route, "h").is_empty());
        assert_eq!(BackendStrategy::test_imports(&direct), "use super::*;\n");
        assert_eq!(
            BackendStrategy::test_fn_signature(&direct, "pets"),
            "fn test_pets() {\n"
        );
        assert_eq!(
            BackendStrategy::test_app_init(&direct, "app"),
            "let _ = 1;\n"
        );
        assert_eq!(
            BackendStrategy::test_body_setup_code(&direct, &body),
            "let _ = 2;\n"
        );
        assert_eq!(
            BackendStrategy::test_request_builder(&direct, "GET", "/", ""),
            "let _ = 3;\n"
        );
        assert_eq!(BackendStrategy::test_api_call(&direct), "let _ = 4;\n");
        assert_eq!(BackendStrategy::test_assertion(&direct), "assert!(true);\n");
        assert!(BackendStrategy::test_validation_helper(&direct).is_empty());
        assert!(BackendStrategy::handler_unit_test(&direct, &route).is_empty());
        assert!(BackendStrategy::generate_custom_test(&direct, &route, "app").is_none());

        for s in [
            &ReqwestStrategy as &dyn BackendStrategy,
            &ClapCliStrategy as &dyn BackendStrategy,
            &McpClientStrategy as &dyn BackendStrategy,
            &McpServerStrategy as &dyn BackendStrategy,
        ] {
            let _ = s.kind();
            let _ = s.extra_cargo_dependencies();
            assert!(!s.handler_imports().is_empty());
            assert!(!s.handler_signature(&route, &[]).is_empty());
            assert!(!s.extra_handler_docs(&route).is_empty());
            assert_eq!(s.path_extractor(&["i32".to_string()]), "i32");
            assert_eq!(
                s.path_extractor(&["i32".to_string(), "String".to_string()]),
                "(i32, String)"
            );
            assert_eq!(s.query_extractor(), "serde_json::Value");
            assert_eq!(s.typed_query_extractor("Q"), "Q");
            assert_eq!(s.query_string_extractor("Q", None), "String");
            assert_eq!(s.header_extractor("H"), "String");
            assert_eq!(s.cookie_extractor(), "String");
            assert_eq!(s.body_extractor("B"), "B");
            assert_eq!(s.form_extractor("F"), "F");
            let _ = s.multipart_extractor("Multipart");
            assert_eq!(s.text_extractor("T"), "String");
            assert_eq!(s.bytes_extractor("Vec<u8>"), "Vec<u8>");
            assert_eq!(s.security_extractor(&[]), "");
            let _ = s.route_registration_statement(&route, "mod::list_pets");
            assert!(!s.test_imports().is_empty());
            assert!(!s.test_fn_signature("pets").is_empty());
            assert!(!s.test_app_init("app").is_empty());
            assert!(!s.test_body_setup_code(&body).is_empty());
            assert!(!s.test_request_builder("GET", "/pets", "").is_empty());
            assert!(!s.test_api_call().is_empty());
            assert!(!s.test_assertion().is_empty());
            let _ = s.test_validation_helper();
            assert!(s.handler_unit_test(&route).is_empty());
            let _ = s.generate_custom_test(&route, "app");
        }

        let cli_main = ClapCliStrategy.generate_cli_main("handlers", &["pet_ops".to_string()]);
        assert!(cli_main.contains("Commands::PetOps"));
        assert_eq!(
            crate::strategies::reqwest::registration::route_registration_statement(&route, "h"),
            ""
        );
        assert_eq!(
            crate::strategies::clap_cli::registration::route_registration_statement(&route, "h"),
            ""
        );
    }
}
