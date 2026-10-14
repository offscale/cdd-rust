#![deny(missing_docs)]

//! Classes Module
//!
//! Provides AST parsing, code emission, diffing, and source patching for Rust structs and enums.

/// Schema diff calculation between Rust source and target IR.
pub mod diff;
/// Rust DTO struct and enum code emission.
pub mod emit;
/// Rust AST parsing into IR models.
pub mod parse;
/// Non-destructive AST source code patching utilities.
pub mod patcher;
