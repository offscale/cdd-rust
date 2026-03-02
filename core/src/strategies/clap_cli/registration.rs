#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Route Registration for Reqwest Client
//!
//! Clients don't register routes, so this is mostly a no-op.

use crate::openapi::parse::ParsedRoute;

/// Generates a no-op statement since clients don't register routes.
pub fn route_registration_statement(_route: &ParsedRoute, _handler_full_path: &str) -> String {
    "".to_string()
}
