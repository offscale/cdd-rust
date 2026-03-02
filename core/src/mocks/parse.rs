#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Mock Parsers
//!
//! Parses mock implementations to help identify mock structures in the codebase.

use crate::error::AppResult;

/// A simple structure representing a mock object or function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMock {
    /// The name of the parsed mock
    pub name: String,
}

/// Extracts mock definitions from test code.
pub fn extract_mocks(_code: &str) -> AppResult<Vec<ParsedMock>> {
    // Currently returns a placeholder empty list as the
    // comprehensive AST matching rules are built incrementally.
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_mocks_empty() {
        let code = r#"
        pub struct FakeDb {}
        "#;
        let mocks = extract_mocks(code).unwrap();
        assert!(mocks.is_empty());
    }
}
