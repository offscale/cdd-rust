#![deny(missing_docs)]

//! # Links
//!
//! Logic for generating runtime code that constructs HATEOAS links
//! based on OpenAPI Link objects.

use crate::oas::models::{LinkParamValue, ParsedLink};
use crate::strategies::actix::utils::{resolve_runtime_expr, to_snake_case};
use url::Url;

/// Helper to generate the code that constructs a specific link URI.
///
/// # Returns
/// A tuple containing:
/// 1. The Rust code block generating the variable.
/// 2. The name of the variable holding the URI string.
pub fn generate_link_construction(link: &ParsedLink) -> (String, String) {
    let var_name = format!("link_{}", to_snake_case(&link.name));
    let mut code = String::new();

    let uri_template = resolve_link_template(link);

    if link.parameters.is_empty() {
        // Static link
        code.push_str(&format!(
            "    let {} = \"{}\";\n",
            var_name,
            escape_rust_string(&uri_template)
        ));
    } else {
        let mut replacements: Vec<(String, String)> = Vec::new();

        for (param_name, value) in &link.parameters {
            let source_var = match value {
                LinkParamValue::Expression(expr) => resolve_runtime_expr(expr),
                LinkParamValue::Literal(lit) => literal_to_rust_expr(lit),
            };
            let template_key = normalize_link_param_key(param_name);
            let placeholder = format!("{{{}}}", template_key);
            if uri_template.contains(&placeholder) {
                replacements.push((placeholder, source_var));
            }
        }

        if !replacements.is_empty() {
            code.push_str(&format!(
                "    let mut {} = \"{}\".to_string();\n",
                var_name,
                escape_rust_string(&uri_template)
            ));
            for (placeholder, source_var) in replacements {
                code.push_str(&format!(
                    "    {} = {}.replace(\"{}\", &format!(\"{{}}\", {}));\n",
                    var_name,
                    var_name,
                    escape_rust_string(&placeholder),
                    source_var
                ));
            }
        } else {
            // Fallback: Parameters defined but not used in template (likely query params or body transfer)
            code.push_str(&format!(
                "    let {} = \"{}\"; // Unused Params: {:?}\n",
                var_name,
                escape_rust_string(&uri_template),
                link.parameters
            ));
        }
    }

    (code, var_name)
}

fn resolve_link_template(link: &ParsedLink) -> String {
    let op_ref = link
        .resolved_operation_ref
        .as_ref()
        .cloned()
        .or_else(|| {
            link.operation_ref.as_ref().and_then(|raw| {
                if raw.starts_with('/') || is_absolute_url(raw) {
                    Some(raw.clone())
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| "/TODO/unknown-path".to_string());

    let Some(server_url) = link.server_url.as_ref() else {
        return op_ref;
    };

    if is_absolute_url(&op_ref) {
        return op_ref;
    }

    join_server_and_path(server_url, &op_ref)
}

/// Normalizes qualified link parameter keys (e.g. `path.id`) to the unqualified name (`id`).
fn normalize_link_param_key(name: &str) -> &str {
    for prefix in ["path.", "query.", "header.", "cookie.", "querystring."] {
        if let Some(stripped) = name.strip_prefix(prefix) {
            return stripped;
        }
    }
    name
}

fn is_absolute_url(value: &str) -> bool {
    Url::parse(value).map(|u| u.has_host()).unwrap_or(false)
}

fn join_server_and_path(server: &str, path: &str) -> String {
    let server = server.trim_end_matches('/');
    let path = path.trim_start_matches('/');

    if server.is_empty() {
        return format!("/{}", path);
    }
    if path.is_empty() {
        return server.to_string();
    }

    format!("{}/{}", server, path)
}

fn literal_to_rust_expr(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", escape_rust_string(s)),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "\"null\"".to_string(),
        _ => format!("\"{}\"", escape_rust_string(&value.to_string())),
    }
}

fn escape_rust_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::RuntimeExpression;
    use crate::oas::models::{LinkParamValue, LinkRequestBody};
    use std::collections::HashMap;

    #[test]
    fn test_generate_static_link() {
        let link = ParsedLink {
            name: "Self".to_string(),
            description: None,
            operation_id: None,
            operation_ref: Some("/users/1".to_string()),
            resolved_operation_ref: None,
            parameters: HashMap::new(),
            request_body: None,
            server: None,
            server_url: None,
        };

        let (code, var_name) = generate_link_construction(&link);
        assert_eq!(var_name, "link_self");
        assert!(code.contains("let link_self = \"/users/1\";"));
    }

    #[test]
    fn test_generate_dynamic_link_with_template() {
        let mut params = HashMap::new();
        params.insert(
            "id".to_string(),
            LinkParamValue::Expression(RuntimeExpression::new("$response.body#/id")),
        );
        let link = ParsedLink {
            name: "User".to_string(),
            description: None,
            operation_id: None,
            operation_ref: Some("/users/{id}".to_string()),
            resolved_operation_ref: None,
            parameters: params,
            request_body: None,
            server: None,
            server_url: None,
        };

        let (code, var_name) = generate_link_construction(&link);
        assert_eq!(var_name, "link_user");
        assert!(code.contains(
            "link_user = link_user.replace(\"{id}\", &format!(\"{}\", response_body.id))"
        ));
    }

    #[test]
    fn test_generate_dynamic_link_fallback_unused_params() {
        let mut params = HashMap::new();
        params.insert(
            "id".to_string(),
            LinkParamValue::Expression(RuntimeExpression::new("$request.path.id")),
        );
        let link = ParsedLink {
            name: "Lookup".to_string(),
            description: None,
            operation_id: None,
            operation_ref: Some("/users".to_string()),
            resolved_operation_ref: None,
            parameters: params,
            request_body: None,
            server: None,
            server_url: None,
        };

        let (code, _var_name) = generate_link_construction(&link);
        assert!(code.contains("Unused Params"));
        assert!(code.contains("/users"));
    }

    #[test]
    fn test_generate_link_default_path() {
        let link = ParsedLink {
            name: "Missing".to_string(),
            description: None,
            operation_id: None,
            operation_ref: None,
            resolved_operation_ref: None,
            parameters: HashMap::new(),
            request_body: None,
            server: None,
            server_url: None,
        };

        let (code, var_name) = generate_link_construction(&link);
        assert_eq!(var_name, "link_missing");
        assert!(code.contains("/TODO/unknown-path"));
    }

    #[test]
    fn test_generate_link_with_server_override() {
        let link = ParsedLink {
            name: "Servered".to_string(),
            description: None,
            operation_id: None,
            operation_ref: Some("/users/{id}".to_string()),
            resolved_operation_ref: None,
            parameters: HashMap::new(),
            request_body: None,
            server: None,
            server_url: Some("https://api.example.com/v1".to_string()),
        };

        let (code, _var_name) = generate_link_construction(&link);
        assert!(code.contains("https://api.example.com/v1/users/{id}"));
    }

    #[test]
    fn test_generate_link_with_server_override_keeps_absolute_ref() {
        let link = ParsedLink {
            name: "Absolute".to_string(),
            description: None,
            operation_id: None,
            operation_ref: Some("https://other.example.com/users/{id}".to_string()),
            resolved_operation_ref: None,
            parameters: HashMap::new(),
            request_body: None,
            server: None,
            server_url: Some("https://api.example.com/v1".to_string()),
        };

        let (code, _var_name) = generate_link_construction(&link);
        assert!(code.contains("https://other.example.com/users/{id}"));
        assert!(!code.contains("https://api.example.com/v1/users/{id}"));
    }

    #[test]
    fn test_generate_dynamic_link_with_literal_param() {
        let mut params = HashMap::new();
        params.insert(
            "id".to_string(),
            LinkParamValue::Literal(serde_json::json!("42")),
        );

        let link = ParsedLink {
            name: "Literal".to_string(),
            description: None,
            operation_id: None,
            operation_ref: Some("/users/{id}".to_string()),
            resolved_operation_ref: None,
            parameters: params,
            request_body: Some(LinkRequestBody::Literal(serde_json::json!({"extra": true}))),
            server: None,
            server_url: None,
        };

        let (code, var_name) = generate_link_construction(&link);
        assert_eq!(var_name, "link_literal");
        assert!(code.contains("replace(\"{id}\", &format!(\"{}\", \"42\"))"));
    }

    #[test]
    fn test_generate_dynamic_link_with_qualified_param_name() {
        let mut params = HashMap::new();
        params.insert(
            "path.id".to_string(),
            LinkParamValue::Expression(RuntimeExpression::new("$request.path.id")),
        );
        let link = ParsedLink {
            name: "Qualified".to_string(),
            description: None,
            operation_id: None,
            operation_ref: Some("/users/{id}".to_string()),
            resolved_operation_ref: None,
            parameters: params,
            request_body: None,
            server: None,
            server_url: None,
        };

        let (code, _var_name) = generate_link_construction(&link);
        assert!(code.contains("replace(\"{id}\", &format!(\"{}\", id))"));
    }
}
