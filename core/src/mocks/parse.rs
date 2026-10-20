#![deny(missing_docs)]

//! # Mock Parsers
//!
//! Parses mock struct and function definitions from Rust source code using `ra_ap_syntax` CST traversal.

use crate::error::AppResult;
use ra_ap_edition::Edition;
use ra_ap_syntax::{
    ast::{self, HasName},
    AstNode, SourceFile,
};

/// A structure representing a discovered mock struct (`Mock*`) or mock handler function (`mock_*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMock {
    /// The identifier name of the parsed mock struct or function.
    pub name: String,
}

/// Extracts mock struct (`Mock*`) and mock function (`mock_*`) definitions from Rust source code.
///
/// # Arguments
/// * `code` - The Rust source code to inspect.
///
/// # Returns
/// * `AppResult<Vec<ParsedMock>>` containing all discovered mock structures and functions.
pub fn extract_mocks(code: &str) -> AppResult<Vec<ParsedMock>> {
    let parse = SourceFile::parse(code, Edition::Edition2021);
    let file = parse.tree();
    let mut mocks = Vec::new();

    for node in file.syntax().descendants() {
        if let Some(name) = ast::Struct::cast(node.clone()).and_then(|s| s.name()) {
            let text = name.text().to_string();
            if text.starts_with("Mock") {
                mocks.push(ParsedMock { name: text });
            }
        } else if let Some(name) = ast::Fn::cast(node).and_then(|f| f.name()) {
            let text = name.text().to_string();
            if text.starts_with("mock_") {
                mocks.push(ParsedMock { name: text });
            }
        }
    }

    Ok(mocks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_mocks_structs_and_functions() {
        let code = r#"
        pub struct FakeDb {}
        pub struct MockPet {
            pub id: i64,
        }
        pub async fn mock_get_pet() -> (u16, &'static str) {
            (200, "{}")
        }
        pub fn regular_fn() {}
        "#;
        let mocks = extract_mocks(code).expect("Failed to extract mocks");
        assert_eq!(
            mocks,
            vec![
                ParsedMock {
                    name: "MockPet".to_string()
                },
                ParsedMock {
                    name: "mock_get_pet".to_string()
                },
            ]
        );
    }
}
