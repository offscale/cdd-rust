#![deny(missing_docs)]

//! # Test Generation and Parsing Module
//!
//! Utilities for emitting and parsing contract and unit test suites.

/// Test emission logic.
pub mod emit;
/// Test parsing logic.
pub mod parse;
/// Runtime test validation support utilities.
pub mod support;
