#![deny(missing_docs)]

//! # Test Parsing
//!
//! Extracts test function names and metadata from Rust source files.

use crate::error::AppResult;
use ra_ap_edition::Edition;
use ra_ap_syntax::{
    ast::{self, HasAttrs, HasName},
    AstNode, SourceFile,
};

/// Extracts the names of all test functions (functions annotated with `#[test]` or `#[*::test]`) from Rust source code.
///
/// # Arguments
/// * `source` - The Rust source code to inspect.
///
/// # Returns
/// * `AppResult<Vec<String>>` containing the discovered test function identifiers.
pub fn extract_test_functions(source: &str) -> AppResult<Vec<String>> {
    let parsed = SourceFile::parse(source, Edition::Edition2021);
    let file = parsed.tree();
    let mut names = Vec::new();

    for func in file.syntax().descendants().filter_map(ast::Fn::cast) {
        let is_test = func.attrs().any(|attr| {
            attr.meta()
                .and_then(|m| m.path())
                .map(|p| {
                    let text = p.syntax().text().to_string();
                    text == "test" || text.ends_with("::test")
                })
                .unwrap_or(false)
        });
        if is_test {
            if let Some(name) = func.name() {
                names.push(name.text().to_string());
            }
        }
    }

    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_test_functions() {
        let code = r#"
        #[test]
        fn unit_test_one() {}

        #[actix_web::test]
        async fn async_test_two() {}

        fn helper_not_a_test() {}
        "#;
        let tests = extract_test_functions(code).expect("must parse");
        assert_eq!(
            tests,
            vec!["unit_test_one".to_string(), "async_test_two".to_string()]
        );
    }
}
