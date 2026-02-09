#![deny(missing_docs)]

//! # Links
//!
//! Logic for generating runtime code that constructs HATEOAS links
//! based on OpenAPI Link objects.

use crate::oas::models::ParsedLink;
use crate::strategies::actix::utils::{resolve_runtime_expr, to_snake_case};

/// Helper to generate the code that constructs a specific link URI.
///
/// # Returns
/// A tuple containing:
/// 1. The Rust code block generating the variable.
/// 2. The name of the variable holding the URI string.
pub fn generate_link_construction(link: &ParsedLink) -> (String, String) {
    let var_name = format!("link_{}", to_snake_case(&link.name));
    let mut code = String::new();

    let uri_template = link
        .operation_ref
        .clone()
        .unwrap_or_else(|| "/TODO/unknown-path".to_string());

    if link.parameters.is_empty() {
        // Static link
        code.push_str(&format!("    let {} = \"{}\";\n", var_name, uri_template));
    } else {
        // Dynamic link formatting
        let mut format_args = Vec::new();
        let rust_template = uri_template.clone();

        for (param_name, expr_obj) in &link.parameters {
            // Resolve the Runtime Expression into a Rust variable accessor / code snippet
            let source_var = resolve_runtime_expr(expr_obj);

            // If the template contains {param_name}, we can use format! args.
            // OAS Rule: Parameters can also be passed to operation args, not just template subst.
            if rust_template.contains(&format!("{{{}}}", param_name)) {
                format_args.push(format!("{} = {}", param_name, source_var));
            }
        }

        if !format_args.is_empty() {
            code.push_str(&format!(
                "    let {} = format!(\"{}\", {});\n",
                var_name,
                rust_template,
                format_args.join(", ")
            ));
        } else {
            // Fallback: Parameters defined but not used in template (likely query params or body transfer)
            code.push_str(&format!(
                "    let {} = \"{}\"; // Unused Params: {:?}\n",
                var_name, uri_template, link.parameters
            ));
        }
    }

    (code, var_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::RuntimeExpression;
    use std::collections::HashMap;

    #[test]
    fn test_generate_static_link() {
        let link = ParsedLink {
            name: "Self".to_string(),
            description: None,
            operation_id: None,
            operation_ref: Some("/users/1".to_string()),
            parameters: HashMap::new(),
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
            RuntimeExpression::new("$response.body#/id"),
        );
        let link = ParsedLink {
            name: "User".to_string(),
            description: None,
            operation_id: None,
            operation_ref: Some("/users/{id}".to_string()),
            parameters: params,
        };

        let (code, var_name) = generate_link_construction(&link);
        assert_eq!(var_name, "link_user");
        assert!(code.contains("format!(\"/users/{id}\""));
        assert!(code.contains("id = response_body.id"));
    }

    #[test]
    fn test_generate_dynamic_link_fallback_unused_params() {
        let mut params = HashMap::new();
        params.insert(
            "id".to_string(),
            RuntimeExpression::new("$request.path.id"),
        );
        let link = ParsedLink {
            name: "Lookup".to_string(),
            description: None,
            operation_id: None,
            operation_ref: Some("/users".to_string()),
            parameters: params,
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
            parameters: HashMap::new(),
        };

        let (code, var_name) = generate_link_construction(&link);
        assert_eq!(var_name, "link_missing");
        assert!(code.contains("/TODO/unknown-path"));
    }
}
