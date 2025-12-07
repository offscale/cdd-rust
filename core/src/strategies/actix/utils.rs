#![deny(missing_docs)]

//! # Utilities
//!
//! Helper functions for case conversion and runtime expression resolution used
//! by the Actix strategy modules.

use crate::oas::models::RuntimeExpression;

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
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            if i > 0 && !result.ends_with('_') {
                result.push('_');
            }
            for lc in c.to_lowercase() {
                result.push(lc);
            }
        } else if c == '-' || c == '/' || c == '.' {
            if !result.ends_with('_') {
                result.push('_');
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Maps OAS Runtime Expressions (OAS 3.2 ABNF) to Rust variable accessors.
///
/// Supported Syntax:
/// - `$url` -> `req.uri().to_string()`
/// - `$method` -> `req.method().as_str()`
/// - `$statusCode` -> `status_code` (Assumes handler variable context)
/// - `$request.query.name` -> `query_name`
/// - `$request.path.name` -> `name` (snake_case)
/// - `$request.header.name` -> `req.headers().get("name")`
/// - `$request.body` -> `body`
/// - `$request.body#/ptr` -> `body.ptr`
/// - `$response.header.name` -> `response_header_name` (Assumes variable)
/// - `$response.body` -> `response_body` (Assumes variable)
/// - `$response.body#/ptr` -> `response_body.ptr`
///
/// Note: Constants (non-expressions) are converted to string literals.
pub fn resolve_runtime_expr(expr_obj: &RuntimeExpression) -> String {
    let expr = expr_obj.as_str();

    // Handle Constants (not starting with $)
    // OAS Spec: "Expressions can be embedded into string values by surrounding the expression with {}"
    // This function handles strict expressions. String interpolation logic belongs in the caller if needed.
    if !expr.starts_with('$') {
        return format!("\"{}\"", expr.replace('"', "\\\""));
    }

    match expr {
        "$url" => "req.uri().to_string()".to_string(),
        "$method" => "req.method().as_str()".to_string(),
        "$statusCode" => "status_code".to_string(),
        _ => resolve_compound_expression(expr),
    }
}

fn resolve_compound_expression(expr: &str) -> String {
    // $request. path | query | header | body
    if let Some(rest) = expr.strip_prefix("$request.") {
        if let Some(path_param) = rest.strip_prefix("path.") {
            return to_snake_case(path_param);
        }
        if let Some(query_param) = rest.strip_prefix("query.") {
            return format!("query_{}", to_snake_case(query_param));
        }
        if let Some(header_name) = rest.strip_prefix("header.") {
            // Headers need access via helper or req object directly
            return format!(
                "req.headers().get(\"{}\").map(|h| h.to_str().unwrap_or_default())",
                header_name
            );
        }
        if rest.starts_with("body") {
            let root = "body";
            if let Some(ptr) = rest.strip_prefix("body#") {
                return resolve_json_pointer(root, ptr);
            }
            return root.to_string();
        }
    }

    // $response. header | body
    if let Some(rest) = expr.strip_prefix("$response.") {
        if let Some(header_name) = rest.strip_prefix("header.") {
            // In a handler context, response headers are usually builder methods.
            // This assumes a variable was bound previously or available in context.
            return format!("response_header_{}", to_snake_case(header_name));
        }
        if rest.starts_with("body") {
            let root = "response_body";
            if let Some(ptr) = rest.strip_prefix("body#") {
                return resolve_json_pointer(root, ptr);
            }
            return root.to_string();
        }
    }

    // Fallback
    format!("/* Unresolved expr: {} */", expr)
}

/// Parses a JSON Pointer (RFC 6901) into a Rust struct/array accessor chain.
///
/// # Logic
/// - Separator `/` becomes `.`
/// - Numeric segments (`/0`) become array indices (`[0]`)
/// - String segments (`/foo`) become fields (`.foo`) in snake_case
/// - Escaped characters (`~1` -> `/`, `~0` -> `~`) are decoded.
fn resolve_json_pointer(root_var: &str, pointer: &str) -> String {
    if pointer.is_empty() {
        return root_var.to_string();
    }

    let mut result = root_var.to_string();
    let segments = pointer.split('/');

    for (i, segment) in segments.enumerate() {
        // RFC 6901: pointers start with /. First split is empty.
        if i == 0 && segment.is_empty() {
            continue;
        }
        if segment.is_empty() {
            continue;
        }

        // Decode escapes
        let decoded = segment.replace("~1", "/").replace("~0", "~");

        if decoded.chars().all(char::is_numeric) {
            result.push_str(&format!("[{}]", decoded));
        } else {
            result.push_str(&format!(".{}", to_snake_case(&decoded)));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::RuntimeExpression;

    fn rx(s: &str) -> RuntimeExpression {
        RuntimeExpression::new(s)
    }

    #[test]
    fn test_resolve_standard_metadata() {
        assert_eq!(resolve_runtime_expr(&rx("$url")), "req.uri().to_string()");
        assert_eq!(
            resolve_runtime_expr(&rx("$method")),
            "req.method().as_str()"
        );
        assert_eq!(resolve_runtime_expr(&rx("$statusCode")), "status_code");
    }

    #[test]
    fn test_resolve_request_sources() {
        assert_eq!(resolve_runtime_expr(&rx("$request.path.userId")), "user_id");
        assert_eq!(
            resolve_runtime_expr(&rx("$request.query.filter")),
            "query_filter"
        );
        assert_eq!(
            resolve_runtime_expr(&rx("$request.header.X-API-KEY")),
            "req.headers().get(\"X-API-KEY\").map(|h| h.to_str().unwrap_or_default())"
        );
    }

    #[test]
    fn test_resolve_response_sources() {
        // Headers assumed snake_case variable binding in codegen scope
        assert_eq!(
            resolve_runtime_expr(&rx("$response.header.Location")),
            "response_header_location"
        );
        assert_eq!(resolve_runtime_expr(&rx("$response.body")), "response_body");
    }

    #[test]
    fn test_resolve_json_pointers() {
        assert_eq!(
            resolve_runtime_expr(&rx("$request.body#/user/id")),
            "body.user.id"
        );
        assert_eq!(
            resolve_runtime_expr(&rx("$response.body#/data/0/id")),
            "response_body.data[0].id"
        );
        // RFC 6901 escape handling
        assert_eq!(
            resolve_runtime_expr(&rx("$request.body#/foo~1bar/baz")),
            "body.foo_bar.baz"
        );
    }

    #[test]
    fn test_constant_fallback() {
        assert_eq!(
            resolve_runtime_expr(&rx("static_value")),
            "\"static_value\""
        );
    }
}
