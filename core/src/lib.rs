#![deny(missing_docs)]

//! # CDD Core
//!
//! Core library for the schema-to-code compiler.

/// Shared error types.
pub mod error;

/// AST Parsing logic.
pub mod parser;

/// Type mapping logic (Rust -> JSON).
pub mod type_mapping;

/// JSON Schema generation.
pub mod schema_generator;

/// Diff calculation.
pub mod diff;

/// Code generation utilities.
pub mod codegen;

/// Code patching utilities.
pub mod patcher;

/// OpenAPI (OAS) parsing utilities.
pub mod oas;

/// Handler Scaffolding Logic.
pub mod handler_generator;

/// Route Registration Logic.
pub mod route_generator;

/// Contract Test Generator Logic.
pub mod contract_test_generator;

/// Strategy Pattern Interfaces.
pub mod strategies;

pub use codegen::{generate_dto, make_record_field};
pub use contract_test_generator::generate_contract_tests_file;
pub use diff::{calculate_diff, Diff};
pub use error::{AppError, AppResult};
pub use handler_generator::update_handler_module;
pub use oas::{
    parse_openapi_document, parse_openapi_document_with_registry, parse_openapi_routes,
    parse_openapi_routes_with_registry, parse_openapi_spec, parse_openapi_spec_with_registry,
    BodyFormat, DocumentRegistry, ParamSource, ParsedOpenApi, ParsedRoute, RequestBodyDefinition,
    RouteParam,
};
pub use parser::{
    extract_struct, extract_struct_fields, extract_struct_names, ParsedField, ParsedStruct,
};
pub use patcher::{add_derive, add_struct_attribute, add_struct_field, modify_struct_field_type};
pub use route_generator::register_routes;
pub use strategies::{ActixStrategy, BackendStrategy};
pub use type_mapping::{JsonSchema, JsonType, RustToJsonMapper, TypeMapper};

/// A placeholder function to verify workspace setup.
pub fn is_operational() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_operational() {
        assert!(is_operational());
    }
}
