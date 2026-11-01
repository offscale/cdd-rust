#![deny(missing_docs)]

//! # Mock Generation
//!
//! Emits strongly-typed mock fixture builders and mock route handlers from IR models.

use crate::classes::parse::models::ParsedStruct;
use crate::error::AppResult;
use crate::openapi::parse::ParsedRoute;

/// Generates mock fixture structs with `sample()` constructors populated from field types.
///
/// # Arguments
/// * `structs` - Slice of [`ParsedStruct`] definitions to generate mock builders for.
///
/// # Returns
/// * `AppResult<String>` containing valid Rust source code for mock fixtures.
pub fn generate_mock_structs(structs: &[ParsedStruct]) -> AppResult<String> {
    let mut out = String::new();
    for s in structs {
        out.push_str(&format!(
            "/// Mock fixture generator for [`{0}`].\n#[derive(Debug, Clone, Default)]\npub struct Mock{0} {{\n",
            s.name
        ));
        for field in &s.fields {
            let escaped = if crate::openapi::parse::models::RustIdent::is_keyword(&field.name) {
                format!("r#{}", field.name)
            } else {
                field.name.clone()
            };
            out.push_str(&format!(
                "    /// Mock field `{}`.\n    pub {}: {},\n",
                field.name, escaped, field.ty
            ));
        }
        out.push_str("}\n\n");
        out.push_str(&format!("impl Mock{} {{\n    /// Constructs a populated mock instance.\n    pub fn sample() -> Self {{\n        Self {{\n", s.name));
        for field in &s.fields {
            let escaped = if crate::openapi::parse::models::RustIdent::is_keyword(&field.name) {
                format!("r#{}", field.name)
            } else {
                field.name.clone()
            };
            let expr = sample_expr_for_type(&field.ty, &field.name);
            out.push_str(&format!("            {}: {},\n", escaped, expr));
        }
        out.push_str("        }\n    }\n}\n");
    }
    Ok(out)
}

/// Generates async mock route handlers returning sample JSON responses based on route metadata.
///
/// # Arguments
/// * `routes` - Slice of [`ParsedRoute`] definitions.
///
/// # Returns
/// * `AppResult<String>` containing valid Rust source code for mock route handlers.
pub fn generate_mock_routes(routes: &[ParsedRoute]) -> AppResult<String> {
    let mut out = String::new();
    for r in routes {
        let status: u16 = r
            .response_status
            .as_deref()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(200);
        let body_json = match &r.response_example {
            Some(ex) => ex.value.to_string(),
            None => format!(
                "{{\"handler\":\"{}\",\"status\":{}}}",
                r.handler_name, status
            ),
        };
        out.push_str(&format!(
            "/// Mock handler for `{}` (`{} {}`).\npub async fn mock_{}() -> (u16, &'static str) {{\n    ({}, {:?})\n}}\n",
            r.handler_name, r.method, r.path, r.handler_name, status, body_json
        ));
    }
    Ok(out)
}

/// Executes `sample_expr_for_type` (sample expr for type).
///
/// # Arguments
/// * `ty` - Input `ty` parameter.
/// * `field_name` - Input `field_name` parameter.
///
/// # Returns
/// Resulting `String` value.
fn sample_expr_for_type(ty: &str, field_name: &str) -> String {
    let trimmed = ty.trim();
    if trimmed.starts_with("Option<") {
        return "None".to_string();
    }
    if trimmed.starts_with("Vec<") {
        return "Vec::new()".to_string();
    }
    match trimmed {
        "String" => format!("\"mock_{}\".to_string()", field_name),
        "bool" => "true".to_string(),
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" => "1".to_string(),
        "f32" | "f64" => "1.0".to_string(),
        _ => "Default::default()".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classes::parse::models::ParsedField;
    use crate::openapi::parse::models::{ExampleValue, RouteKind};
    use ra_ap_edition::Edition;
    use ra_ap_syntax::SourceFile;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn test_generate_mock_routes_and_structs_valid_ast() {
        let route = ParsedRoute {
            path: "/ping".to_string(),
            summary: None,
            description: None,
            path_summary: None,
            path_description: None,
            operation_summary: None,
            operation_description: None,
            path_extensions: BTreeMap::new(),
            base_path: None,
            path_servers: None,
            servers_override: None,
            method: "GET".to_string(),
            handler_name: "ping".to_string(),
            operation_id: None,
            params: vec![],
            path_params: vec![],
            request_body: None,
            raw_request_body: None,
            tags: vec![],
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: Some("201".to_string()),
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            raw_responses: None,
            response_links: None,
            callbacks: vec![],
            kind: RouteKind::Path,
            extensions: BTreeMap::new(),
            external_docs: None,
            deprecated: false,
        };
        let mut route_data = route.clone();
        route_data.handler_name = "ping_data".to_string();
        route_data.response_example = Some(ExampleValue::data(json!({"ok": true})));

        let mut route_raw = route.clone();
        route_raw.handler_name = "ping_raw".to_string();
        route_raw.response_status = None;
        route_raw.response_example = Some(ExampleValue::serialized(json!("pong")));

        let code = generate_mock_routes(&[route, route_data, route_raw])
            .expect("Failed to generate mock routes");
        assert!(code.contains("pub async fn mock_ping() -> (u16, &'static str)"));
        assert!(code.contains("(201,"));
        assert!(code.contains("mock_ping_data"));
        assert!(code.contains("mock_ping_raw"));
        let parsed_routes = SourceFile::parse(&code, Edition::Edition2021);
        assert!(parsed_routes.errors().is_empty());

        let s = ParsedStruct {
            name: "Pet".to_string(),
            description: None,
            rename: None,
            rename_all: None,
            fields: vec![
                ParsedField {
                    name: "id".to_string(),
                    ty: "i64".to_string(),
                    description: None,
                    rename: None,
                    is_skipped: false,
                    is_read_only: false,
                    is_write_only: false,
                    is_deprecated: false,
                    external_docs: None,
                },
                ParsedField {
                    name: "name".to_string(),
                    ty: "String".to_string(),
                    description: None,
                    rename: None,
                    is_skipped: false,
                    is_read_only: false,
                    is_write_only: false,
                    is_deprecated: false,
                    external_docs: None,
                },
                ParsedField {
                    name: "active".to_string(),
                    ty: "bool".to_string(),
                    description: None,
                    rename: None,
                    is_skipped: false,
                    is_read_only: false,
                    is_write_only: false,
                    is_deprecated: false,
                    external_docs: None,
                },
                ParsedField {
                    name: "weight".to_string(),
                    ty: "f64".to_string(),
                    description: None,
                    rename: None,
                    is_skipped: false,
                    is_read_only: false,
                    is_write_only: false,
                    is_deprecated: false,
                    external_docs: None,
                },
                ParsedField {
                    name: "tags".to_string(),
                    ty: "Vec<String>".to_string(),
                    description: None,
                    rename: None,
                    is_skipped: false,
                    is_read_only: false,
                    is_write_only: false,
                    is_deprecated: false,
                    external_docs: None,
                },
                ParsedField {
                    name: "nickname".to_string(),
                    ty: "Option<String>".to_string(),
                    description: None,
                    rename: None,
                    is_skipped: false,
                    is_read_only: false,
                    is_write_only: false,
                    is_deprecated: false,
                    external_docs: None,
                },
                ParsedField {
                    name: "type".to_string(),
                    ty: "CustomCategory".to_string(),
                    description: None,
                    rename: None,
                    is_skipped: false,
                    is_read_only: false,
                    is_write_only: false,
                    is_deprecated: false,
                    external_docs: None,
                },
            ],
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
        };
        let structs_code = generate_mock_structs(&[s]).expect("Failed to generate mock structs");
        assert!(structs_code.contains("pub struct MockPet"));
        assert!(structs_code.contains("pub fn sample() -> Self"));
        assert!(structs_code.contains("name: \"mock_name\".to_string()"));
        assert!(structs_code.contains("r#type: Default::default()"));
        let parsed_structs = SourceFile::parse(&structs_code, Edition::Edition2021);
        assert!(parsed_structs.errors().is_empty());
    }
}
