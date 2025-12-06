#![deny(missing_docs)]

//! # Handler Parsing Utilities
//!
//! Low-level parsing functions using `ra_ap_syntax` to interact with existing code,
//! and helpers for parsing route paths.

use crate::oas::{ParamSource, ParsedRoute};
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, SourceFile};
use regex::Regex;
use std::collections::HashSet;

/// Parses the source using rust-analyzer syntax tree to find all function names.
pub(crate) fn extract_fn_names(source: &str) -> HashSet<String> {
    let parse = SourceFile::parse(source, Edition::Edition2021);
    parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter_map(|f| f.name().map(|n| n.text().to_string()))
        .collect()
}

/// Extracts parameter names from a path template like `/users/{id}`.
pub(crate) fn extract_path_vars(path: &str) -> Vec<String> {
    let re = Regex::new(r"\{([^}]+)}").expect("Invalid regex constant");
    re.captures_iter(path).map(|c| c[1].to_string()).collect()
}

/// Helper to lookup a parameter type from the parsed route definition.
pub(crate) fn find_param_type(
    route: &ParsedRoute,
    name: &str,
    source: ParamSource,
) -> Option<String> {
    route
        .params
        .iter()
        .find(|p| p.name == name && p.source == source)
        .map(|p| p.ty.clone())
}

/// Converts a string to snake_case for use as a variable name.
pub(crate) fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                let prev = chars[i - 1];
                if prev != '-' && prev != '_' && !prev.is_uppercase() {
                    result.push('_');
                }
            }
            for lc in c.to_lowercase() {
                result.push(lc);
            }
        } else {
            result.push(c);
        }
    }
    result.replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_fn_names() {
        let code = r#"
            pub async fn get_user() {}
            fn internal() {}
        "#;
        let names = extract_fn_names(code);
        assert!(names.contains("get_user"));
        assert!(names.contains("internal"));
    }

    #[test]
    fn test_snake_case_conversion() {
        assert_eq!(to_snake_case("userId"), "user_id");
        assert_eq!(to_snake_case("id"), "id");
        assert_eq!(to_snake_case("camelCaseTemp"), "camel_case_temp");
        assert_eq!(to_snake_case("X-Forwarded-For"), "x_forwarded_for");
    }
}
