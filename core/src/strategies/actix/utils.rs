#![deny(missing_docs)]

//! # Utilities
//!
//! Helper functions for case conversion and runtime expression resolution used
//! by the Actix strategy modules.

/// Converts a string to PascalCase.
pub fn to_pascal_case(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut c = part.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}

/// Converts a string to snake_case.
///
/// Handles CamelCase inputs as well as separators like `-`, `/`, and `.`.
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            // Append underscore if not start and previous was not already a separator
            if i > 0 && !result.ends_with('_') {
                result.push('_');
            }
            for lc in c.to_lowercase() {
                result.push(lc);
            }
        } else if c == '-' || c == '/' || c == '.' {
            // Treat standard delimiters as underscores
            if !result.ends_with('_') {
                result.push('_');
            }
        } else {
            result.push(c);
        }
    }
    // Clean up purely alphanumeric check could leave trailing _, but for identifiers logic usually assumes valid input.
    result
}

/// Maps OAS runtime expression to Rust variable.
///
/// # Logic
/// * `$request.path.x` -> `x` (snake_case)
/// * `$request.query.x` -> `query_x`
/// * `$request.body#/x` -> `body.x`
pub fn resolve_runtime_expr(expr: &str) -> String {
    if let Some(stripped) = expr.strip_prefix("$request.path.") {
        // e.g. $request.path.id -> id
        to_snake_case(stripped)
    } else if let Some(stripped) = expr.strip_prefix("$request.query.") {
        // e.g. $request.query.q -> query_q
        format!("query_{}", to_snake_case(stripped))
    } else if let Some(stripped) = expr.strip_prefix("$request.body#/") {
        // e.g. $request.body#/data/id -> body.data_id
        format!("body.{}", to_snake_case(stripped))
    } else {
        // Fallback or Unknown
        format!("/* {} */", expr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_runtime_expr() {
        assert_eq!(resolve_runtime_expr("$request.path.userId"), "user_id");
        assert_eq!(
            resolve_runtime_expr("$request.query.filter"),
            "query_filter"
        );
        // This test case triggered the panic previously due to unhandled '/'
        assert_eq!(
            resolve_runtime_expr("$request.body#/data/id"),
            "body.data_id"
        );
    }

    #[test]
    fn test_pascal_case() {
        assert_eq!(to_pascal_case("api_key"), "ApiKey");
        assert_eq!(to_pascal_case("oauth2"), "Oauth2");
        assert_eq!(to_pascal_case("content-type"), "ContentType");
    }

    #[test]
    fn test_snake_case_complex() {
        assert_eq!(to_snake_case("camelCase"), "camel_case");
        assert_eq!(to_snake_case("PascalCase"), "pascal_case");
        assert_eq!(to_snake_case("kebab-case"), "kebab_case");
        assert_eq!(to_snake_case("path/segment"), "path_segment");
    }
}
