#![deny(missing_docs)]

//! # Naming Utilities
//!
//! Helper functions for deriving Rust-safe handler names from OpenAPI paths and Operation IDs.

/// Converts a mixed-case string (CamelCase or camelCase) to snake_case.
/// Used for converting `operationId` into valid Rust function names.
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            for lc in c.to_lowercase() {
                result.push(lc);
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Derives a handler name from the HTTP Method and URL path when `operationId` is missing.
///
/// e.g. `GET /users/{id}` -> `get_users_id`
pub fn derive_handler_name(method: &str, path: &str) -> String {
    let clean_path = path.replace(['{', '}'], "").replace('/', "_");
    format!(
        "{}_{}",
        method.to_lowercase(),
        clean_path.trim_start_matches('_')
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snake_case_conversion() {
        assert_eq!(to_snake_case("userId"), "user_id");
        assert_eq!(to_snake_case("id"), "id");
        assert_eq!(to_snake_case("camelCaseTemp"), "camel_case_temp");
        assert_eq!(to_snake_case("GetUsers"), "get_users");
    }

    #[test]
    fn test_derive_handler_name() {
        assert_eq!(derive_handler_name("GET", "/users"), "get_users");
        assert_eq!(
            derive_handler_name("POST", "/users/{id}/activate"),
            "post_users_id_activate"
        );
    }
}
