#![deny(missing_docs)]

//! # Handler Parsing Utilities
//!
//! Low-level parsing functions using `ra_ap_syntax` to interact with existing code,
//! and helpers for parsing route paths.

use crate::openapi::parse::{ParamSource, ParsedRoute};
use ra_ap_edition::Edition;
use ra_ap_syntax::ast::{self, HasName};
use ra_ap_syntax::{AstNode, SourceFile};
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

/// Parses the source using rust-analyzer syntax tree to find all struct names.
pub(crate) fn extract_struct_names(source: &str) -> HashSet<String> {
    let parse = SourceFile::parse(source, Edition::Edition2021);
    parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Struct::cast)
        .filter_map(|s| s.name().map(|n| n.text().to_string()))
        .collect()
}

/// Extracts parameter names from a path template like `/users/{id}`.
pub(crate) fn extract_path_vars(path: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let after_open = &rest[start + 1..];
        let Some(end) = after_open.find('}') else {
            break;
        };
        let var_name = &after_open[..end];
        if !var_name.is_empty() {
            vars.push(var_name.to_string());
        }
        rest = &after_open[end + 1..];
    }
    vars
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

/// Converts a string to PascalCase for use as a type name.
pub(crate) fn to_pascal_case(s: &str) -> String {
    let mut out = String::new();
    let mut capitalize = true;

    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            if capitalize {
                out.extend(ch.to_uppercase());
                capitalize = false;
            } else {
                out.extend(ch.to_lowercase());
            }
        } else {
            capitalize = true;
        }
    }

    if out.is_empty() {
        out.push_str("Query");
    }

    if out
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        out.insert(0, 'Q');
    }

    out
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
        assert_eq!(to_snake_case("HTTPServer"), "httpserver");
        assert_eq!(to_snake_case("user_Id"), "user_id");
    }

    #[test]
    fn test_extract_struct_names() {
        let code = r#"
            pub struct QueryParams {}
            struct Internal;
        "#;
        let names = extract_struct_names(code);
        assert!(names.contains("QueryParams"));
        assert!(names.contains("Internal"));
    }

    #[test]
    fn test_pascal_case_conversion() {
        assert_eq!(to_pascal_case("get_user"), "GetUser");
        assert_eq!(to_pascal_case("querystring"), "Querystring");
        assert_eq!(to_pascal_case("x-token"), "XToken");
        assert_eq!(to_pascal_case("---"), "Query");
        assert_eq!(to_pascal_case("123_items"), "Q123Items");
        assert_eq!(
            extract_path_vars("/users/{id}/{}/items/{unclosed"),
            vec!["id".to_string()]
        );
    }
}
