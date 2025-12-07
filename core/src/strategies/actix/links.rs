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
        // Dynamic link: format!(".../{id}", id = ...)
        let mut format_args = Vec::new();
        let rust_template = uri_template.clone();

        for (param_name, expr) in &link.parameters {
            let source_var = resolve_runtime_expr(expr);

            // If the template contains {param_name}, we can use format! args.
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
            code.push_str(&format!(
                "    let {} = \"{}\"; // Params: {:?}\n",
                var_name, uri_template, link.parameters
            ));
        }
    }

    (code, var_name)
}
