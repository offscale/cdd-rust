#![deny(missing_docs)]

//! # Handler Generator Module
//!
//! Generates Actix Web handler functions from parsed OpenAPI routes.
//! This module scaffolds the Rust code required to handle HTTP requests,
//! including resolving path parameters, query strings, headers, cookies, and request bodies
//! into strictly typed extractors.

mod builder;
mod extractors;
mod parsing;

pub use builder::update_handler_module;
