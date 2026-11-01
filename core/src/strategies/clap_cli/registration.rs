#![deny(missing_docs)]

//! # Route Registration for Reqwest Client
//!
//! Clients don't register routes, so this is mostly a no-op.

use crate::openapi::parse::ParsedRoute;

/// Generates a no-op statement since clients don't register routes.
///
/// # Arguments
/// * `route` - Input `route` parameter.
/// * `handler_full_path` - Input `handler_full_path` parameter.
///
/// # Returns
/// Resulting `String` value.
pub fn route_registration_statement(_route: &ParsedRoute, _handler_full_path: &str) -> String {
    "".to_string()
}
