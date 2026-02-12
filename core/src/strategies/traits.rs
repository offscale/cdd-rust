#![deny(missing_docs)]

//! # Backend Strategy Trait
//!
//! Defines the interface required to generate code for a specific web framework
//! (e.g. Actix, Axum, Poem, etc.).

use crate::oas::models::{ParsedLink, ResponseHeader, SecurityRequirement};
use crate::oas::ParsedRoute;
use crate::oas::RequestBodyDefinition;

/// A strategy trait for decoupling framework-specific code generation.
///
/// Implementors define how to generate imports, handler signatures, extractors,
/// and route registrations for a specific backend.
/// Also includes methods for generating integration tests.
pub trait BackendStrategy {
    // --- Scaffolding & Routing ---

    /// Returns the standard imports for a handler file in this framework.
    fn handler_imports(&self) -> String;

    /// Generates a handler function signature.
    ///
    /// # Arguments
    ///
    /// * `func_name` - The name of the function.
    /// * `args` - A list of argument declaration strings (e.g. `id: web::Path<Uuid>`).
    /// * `response_type` - The specific return type if identified (e.g. `UserResponse`).
    /// * `response_headers` - List of headers the response is expected to include.
    /// * `response_links` - List of links associated with the response (HATEOAS).
    fn handler_signature(
        &self,
        func_name: &str,
        args: &[String],
        response_type: Option<&str>,
        response_headers: &[ResponseHeader],
        response_links: Option<&[ParsedLink]>,
    ) -> String;

    /// Generates the type string for path parameter extraction.
    ///
    /// # Arguments
    ///
    /// * `inner_types` - The Rust types of the path parameters (e.g. `["Uuid", "i32"]`).
    fn path_extractor(&self, inner_types: &[String]) -> String;

    /// Generates the type string for query parameter extraction.
    fn query_extractor(&self) -> String;

    /// Generates the type string for strongly typed query parameter extraction.
    ///
    /// # Arguments
    ///
    /// * `inner_type` - The Rust struct type representing query parameters.
    fn typed_query_extractor(&self, inner_type: &str) -> String {
        let _ = inner_type;
        self.query_extractor()
    }

    /// Generates the type string for the entire Query String extraction (OAS 3.2).
    ///
    /// # Arguments
    ///
    /// * `inner_type` - The specific type the entire query string should map to.
    fn query_string_extractor(&self, inner_type: &str) -> String;

    /// Generates the type string for Header parameter extraction.
    fn header_extractor(&self, inner_type: &str) -> String;

    /// Generates the type string for Cookie parameter extraction.
    fn cookie_extractor(&self) -> String;

    /// Generates the type string for JSON request body extraction.
    ///
    /// # Arguments
    ///
    /// * `body_type` - The Rust type of the body (e.g. `CreateUserRequest`).
    fn body_extractor(&self, body_type: &str) -> String;

    /// Generates the type string for Form request body extraction.
    ///
    /// # Arguments
    ///
    /// * `body_type` - The Rust type of the body (e.g. `SearchForm`).
    fn form_extractor(&self, body_type: &str) -> String;

    /// Generates the type string for Multipart request body extraction.
    ///
    /// # Arguments
    ///
    /// * `body_type` - The Rust type of the body (e.g. `UploadForm`).
    fn multipart_extractor(&self, body_type: &str) -> String;

    /// Generates the type string for Text request body extraction.
    ///
    /// # Arguments
    ///
    /// * `body_type` - The Rust type of the body (e.g. `String`).
    fn text_extractor(&self, body_type: &str) -> String;

    /// Generates the type string for Binary request body extraction.
    ///
    /// # Arguments
    ///
    /// * `body_type` - The Rust type of the body (e.g. `Vec<u8>`).
    fn bytes_extractor(&self, body_type: &str) -> String;

    /// Generates the type string for a Security extraction/Guard.
    /// Used when `security: [{...}]` is present.
    /// Expects a placeholder type name (e.g. `UserPrincipal` or generic `Auth`)
    /// based on scheme.
    fn security_extractor(&self, requirements: &[SecurityRequirement]) -> String;

    /// Generates the route registration code statement.
    ///
    /// # Arguments
    ///
    /// * `route` - The parsed route definition.
    /// * `handler_full_path` - The fully qualified path to the handler (e.g. `handlers::users::create`).
    fn route_registration_statement(&self, route: &ParsedRoute, handler_full_path: &str) -> String;

    // --- Test Generation ---

    /// Returns the standard imports for a test file in this framework.
    fn test_imports(&self) -> String;

    /// Returns the test function signature (including attributes).
    ///
    /// Example: `#[actix_web::test]\nasync fn test_foo() {`
    fn test_fn_signature(&self, fn_name: &str) -> String;

    /// Returns code to initialize the application for testing.
    ///
    /// * `app_factory` - code string for the app factory (e.g. `crate::create_app`).
    fn test_app_init(&self, app_factory: &str) -> String;

    /// Returns the code snippet that attaches a dummy body to the request.
    ///
    /// * `body` - The parsed request body definition.
    fn test_body_setup_code(&self, body: &RequestBodyDefinition) -> String;

    /// Returns code to build the request object.
    ///
    /// * `method` - HTTP method (GET, POST).
    /// * `uri` - Request URI.
    /// * `body_setup` - Code snippet inserted if body is present.
    fn test_request_builder(&self, method: &str, uri: &str, body_setup: &str) -> String;

    /// Returns code to execute the request against the app.
    fn test_api_call(&self) -> String;

    /// Returns assertion code for the response.
    fn test_assertion(&self) -> String;

    /// Returns the helper function code for validating responses against OpenAPI.
    fn test_validation_helper(&self) -> String;
}
