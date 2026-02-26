#![deny(missing_docs)]

//! # CDD Core
//!
//! Core library for the schema-to-code compiler.

/// Shared error types.
pub mod error;

/// Type mapping logic (Rust -> JSON).
pub mod type_mapping;

/// Strategy Pattern Interfaces.
pub mod strategies;

/// OpenAPI (OAS) parsing and emitting.
pub mod openapi;

/// Classes (Models/Structs) parsing and emitting.
pub mod classes;

/// Functions (Handlers) parsing and emitting.
pub mod functions;

/// Routes parsing and emitting.
pub mod routes;

/// Tests parsing and emitting.
pub mod tests;

/// Mocks parsing and emitting.
pub mod mocks;

/// Docstrings parsing and emitting.
pub mod docstrings;

pub use classes::diff::{calculate_diff, Diff};
pub use classes::emit::{generate_dto, make_record_field};
pub use classes::parse::{
    extract_struct, extract_struct_fields, extract_struct_names, ParsedField, ParsedStruct,
};
pub use classes::patcher::{
    add_derive, add_struct_attribute, add_struct_field, modify_struct_field_type,
};
pub use error::{AppError, AppResult};
pub use functions::emit::update_handler_module;
pub use openapi::parse::{
    parse_openapi_document, parse_openapi_document_with_registry, parse_openapi_routes,
    parse_openapi_routes_with_registry, parse_openapi_spec, parse_openapi_spec_with_registry,
    BodyFormat, DocumentRegistry, ParamSource, ParsedOpenApi, ParsedRoute, RequestBodyDefinition,
    RouteParam,
};
pub use routes::emit::register_routes;
pub use strategies::{ActixStrategy, BackendStrategy};
pub use tests::emit::generate_contract_tests_file;
pub use type_mapping::{JsonSchema, JsonType, RustToJsonMapper, TypeMapper};

/// A placeholder function to verify workspace setup.
pub fn is_operational() -> bool {
    true
}

#[cfg(test)]
mod tests_local {
    use super::*;

    #[test]
    fn test_is_operational() {
        assert!(is_operational());
    }
}
