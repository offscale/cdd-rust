#![deny(missing_docs)]

//! Routes Module
//!
//! Parses and emits framework route registration code.

/// Route registration code generator and AST injector.
pub mod emit;
/// Route registration and attribute AST parser.
pub mod parse;
