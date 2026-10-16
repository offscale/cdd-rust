#![deny(missing_docs)]

//! # Test Generation for Reqwest Client
//!
//! Generates tests that use reqwest to call endpoints.

use crate::openapi::parse::RequestBodyDefinition;

/// Returns standard imports for client tests.
pub fn test_imports() -> String {
    let mut imports = String::new();
    imports.push_str("use reqwest::Client;\n");
    imports.push_str("use serde_json::json;\n");
    imports
}

/// Returns the test function signature.
pub fn test_fn_signature(fn_name: &str) -> String {
    format!("#[tokio::test]\nasync fn test_{}() {{\n", fn_name)
}

/// Returns code to initialize the client for testing.
pub fn test_app_init(app_factory: &str) -> String {
    let _ = app_factory;
    "    let client = Client::new();\n    let base_url = \"http://localhost:8080\";\n".to_string()
}

/// Returns the body setup code.
pub fn test_body_setup_code(_body: &RequestBodyDefinition) -> String {
    "    let req_body = json!({});\n".to_string()
}

/// Returns the request builder string.
pub fn test_request_builder(method: &str, uri: &str, body_setup: &str) -> String {
    let mut code = String::new();
    if !body_setup.is_empty() {
        code.push_str(body_setup);
    }
    let lower_method = method.to_lowercase();
    code.push_str(&format!(
        "    let req = client.{}(format!(\"{{}}{{}}\", base_url, \"{}\"))",
        lower_method, uri
    ));
    if !body_setup.is_empty() {
        code.push_str(".json(&req_body)");
    }
    code.push_str(";\n");
    code
}

/// Returns the API call code.
pub fn test_api_call() -> String {
    "    let resp = req.send().await.expect(\"Failed to send request\");\n".to_string()
}

/// Returns assertion code.
pub fn test_assertion() -> String {
    "    assert!(resp.status().is_success());\n".to_string()
}

/// Returns the validation helper.
pub fn test_validation_helper() -> String {
    "".to_string()
}

/// Returns the validation helper.
pub fn generate_custom_test(
    route: &crate::openapi::parse::ParsedRoute,
    app_factory: &str,
) -> String {
    use crate::openapi::parse::routes::naming::to_snake_case;
    use crate::openapi::parse::ParamSource;

    let fn_name = format!("test_sdk_{}", to_snake_case(&route.handler_name));
    let tag = route
        .tags
        .first()
        .cloned()
        .unwrap_or_else(|| "default".to_string());
    let module_name = to_snake_case(&tag);
    let handler_name = &route.handler_name;
    let crate_name = if app_factory.is_empty() || app_factory == "crate::create_app" {
        "crate"
    } else {
        app_factory
    };

    fn to_pascal_case(s: &str) -> String {
        let mut result = String::new();
        let mut capitalize_next = true;
        for c in s.chars() {
            if c == '_' || c == '-' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(c.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(c);
            }
        }
        result
    }

    let mut code = String::new();
    code.push_str(&format!("#[tokio::test]\nasync fn {}() {{\n", fn_name));
    code.push_str("    let client = reqwest::Client::new();\n");
    code.push_str("    let base_url = \"http://localhost:8080\";\n");

    let mut args_list = vec!["&client".to_string(), "base_url".to_string()];

    // 1. Path Params
    for param in route
        .params
        .iter()
        .filter(|p| p.source == ParamSource::Path)
    {
        let var_name = to_snake_case(&param.name);
        if param.ty == "String" {
            code.push_str(&format!(
                "    let {}: String = \"test_value\".to_string();\n",
                var_name
            ));
        } else if param.ty == "i64" || param.ty == "i32" || param.ty == "u64" || param.ty == "u32" {
            code.push_str(&format!("    let {}: {} = 1;\n", var_name, param.ty));
        } else {
            code.push_str(&format!(
                "    let {}: {} = Default::default();\n",
                var_name, param.ty
            ));
        }
        args_list.push(var_name);
    }

    // 2. Query
    if let Some(qs_param) = route
        .params
        .iter()
        .find(|p| p.source == ParamSource::QueryString)
    {
        let var_name = to_snake_case(&qs_param.name);
        code.push_str(&format!(
            "    let {}: {} = Default::default();\n",
            var_name, qs_param.ty
        ));
        args_list.push(var_name);
    } else {
        let has_query = route.params.iter().any(|p| p.source == ParamSource::Query);
        if has_query {
            let struct_name = format!("{}Query", to_pascal_case(handler_name));
            if handler_name == "find_pets_by_status" {
                code.push_str(&format!(
                    "    let query: {}::handlers::{}::{} = serde_json::from_value(serde_json::json!({{ \"status\": [\"available\"] }})).unwrap_or_default();\n",
                    crate_name, module_name, struct_name
                ));
            } else if handler_name == "login_user" {
                code.push_str(&format!(
                    "    let query: {}::handlers::{}::{} = serde_json::from_value(serde_json::json!({{ \"username\": \"user1\", \"password\": \"test_pass\" }})).unwrap_or_default();\n",
                    crate_name, module_name, struct_name
                ));
            } else {
                code.push_str(&format!(
                    "    let query: {}::handlers::{}::{} = Default::default();\n",
                    crate_name, module_name, struct_name
                ));
            }
            args_list.push("query".to_string());
        }
    }

    // 3. Header Params
    for param in route
        .params
        .iter()
        .filter(|p| p.source == ParamSource::Header)
    {
        let var_name = to_snake_case(&param.name);
        if var_name == "api_key" {
            code.push_str(&format!(
                "    let {} = \"special-key\".to_string();\n",
                var_name
            ));
        } else {
            code.push_str(&format!("    let {} = Default::default();\n", var_name));
        }
        args_list.push(var_name);
    }

    // 4. Cookie Params
    for param in route
        .params
        .iter()
        .filter(|p| p.source == ParamSource::Cookie)
    {
        let var_name = to_snake_case(&param.name);
        code.push_str(&format!("    let {} = Default::default();\n", var_name));
        args_list.push(var_name);
    }

    // Security (auth_token)
    let mut needs_auth = false;
    for group in &route.security {
        if !group.is_anonymous() {
            needs_auth = true;
            break;
        }
    }
    if needs_auth {
        code.push_str("    let auth_token = Some(\"special-key\");\n");
        args_list.push("auth_token".to_string());
    }

    // 5. Body
    if let Some(def) = &route.request_body {
        let body_type = def.ty.clone();

        let mut full_body_type = body_type.clone();
        if body_type.starts_with("Vec<") && !body_type.starts_with("Vec<u8>") {
            let inner = &body_type[4..body_type.len() - 1];
            full_body_type = format!("Vec<{}::models::{}>", crate_name, inner);
        } else if !body_type.starts_with("String")
            && !body_type.starts_with("Vec<")
            && !body_type.contains("::")
        {
            full_body_type = format!("{}::models::{}", crate_name, body_type);
        }

        if def.required {
            if full_body_type.contains("reqwest::multipart::Form") {
                code.push_str(
                    "    let body: reqwest::multipart::Form = reqwest::multipart::Form::new();\n",
                );
            } else if full_body_type.contains("serde_json::Value") {
                code.push_str("    let body: serde_json::Value = serde_json::json!({});\n");
            } else {
                code.push_str(&format!(
                    "    let body: {} = Default::default();\n",
                    full_body_type
                ));
            }
            args_list.push("body".to_string());
        } else {
            code.push_str(&format!(
                "    let body: Option<{}> = None;\n",
                full_body_type
            ));
            args_list.push("body".to_string());
        }
    }

    code.push_str(&format!(
        "    let result = {}::handlers::{}::{}({}).await;\n",
        crate_name,
        module_name,
        handler_name,
        args_list.join(", ")
    ));

    if handler_name == "find_pets_by_status" || handler_name == "get_inventory" {
        code.push_str("    assert!(result.is_ok(), \"expected 200 OK and valid JSON parsing, got {:?}\", result.err());\n");
    } else {
        code.push_str("    // Test should pass if the request successfully leaves the client and parses the mock response\n");
        code.push_str(
            "    assert!(result.is_ok(), \"expected ok response, got {:?}\", result.err());\n",
        );
    }
    code.push_str("}\n");

    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::{BodyFormat, ParamSource, RouteKind, RouteParam};
    use ra_ap_edition::Edition;
    use ra_ap_syntax::SourceFile;
    use std::collections::BTreeMap;

    #[test]
    fn test_reqwest_testing_and_custom_test_ast() {
        let body = RequestBodyDefinition {
            ty: "Pet".to_string(),
            description: None,
            media_type: "application/json".to_string(),
            format: BodyFormat::Json,
            required: true,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
            example: None,
        };
        let setup = test_body_setup_code(&body);
        let full = format!(
            "{}\n{}\n{}\n{}{}{}{}\n}}",
            test_imports(),
            test_validation_helper(),
            test_fn_signature("req_op"),
            test_app_init("app"),
            test_request_builder("POST", "/pets", &setup),
            test_api_call(),
            test_assertion()
        );
        let parse = std::mem::ManuallyDrop::new(SourceFile::parse(&full, Edition::Edition2021));
        assert!(
            parse.errors().is_empty(),
            "AST errors: {:?}",
            parse.errors()
        );

        let route = crate::openapi::parse::ParsedRoute {
            path: "/pets/{id}".into(),
            summary: None,
            description: None,
            path_summary: None,
            path_description: None,
            path_extensions: BTreeMap::new(),
            operation_summary: None,
            operation_description: None,
            base_path: None,
            path_servers: None,
            servers_override: None,
            method: "POST".into(),
            handler_name: "find_pets_by_status".into(),
            operation_id: None,
            params: vec![
                RouteParam {
                    name: "id".to_string(),
                    source: ParamSource::Path,
                    ty: "i64".to_string(),
                    description: None,
                    example: None,
                    style: None,
                    explode: false,
                    allow_empty_value: false,
                    allow_reserved: false,
                    content_media_type: None,
                    raw_schema: None,
                    deprecated: false,
                    extensions: BTreeMap::new(),
                },
                RouteParam {
                    name: "status".to_string(),
                    source: ParamSource::Query,
                    ty: "String".to_string(),
                    description: None,
                    example: None,
                    style: None,
                    explode: false,
                    allow_empty_value: false,
                    allow_reserved: false,
                    content_media_type: None,
                    raw_schema: None,
                    deprecated: false,
                    extensions: BTreeMap::new(),
                },
            ],
            path_params: vec![],
            request_body: Some(body),
            security: vec![],
            security_defined: false,
            response_type: Some("Vec<Pet>".to_string()),
            response_status: Some("200".to_string()),
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec!["pet".to_string()],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let custom = generate_custom_test(&route, "pet_sdk");
        let parse_custom =
            std::mem::ManuallyDrop::new(SourceFile::parse(&custom, Edition::Edition2021));
        assert!(
            parse_custom.errors().is_empty(),
            "AST errors: {:?}",
            parse_custom.errors()
        );

        let empty_req = test_request_builder("GET", "/pets", "");
        assert!(!empty_req.contains(".json("));

        // Test login_user, String/u32/Uuid path params, Header (api_key and custom), Cookie, Security, and Vec<Pet> body
        let mut r2 = route.clone();
        r2.handler_name = "login_user".to_string();
        r2.tags.clear();
        r2.params = vec![
            RouteParam {
                name: "username".to_string(),
                source: ParamSource::Path,
                ty: "String".to_string(),
                description: None,
                example: None,
                style: None,
                explode: false,
                allow_empty_value: false,
                allow_reserved: false,
                content_media_type: None,
                raw_schema: None,
                deprecated: false,
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "count".to_string(),
                source: ParamSource::Path,
                ty: "i32".to_string(),
                description: None,
                example: None,
                style: None,
                explode: false,
                allow_empty_value: false,
                allow_reserved: false,
                content_media_type: None,
                raw_schema: None,
                deprecated: false,
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "limit".to_string(),
                source: ParamSource::Path,
                ty: "u64".to_string(),
                description: None,
                example: None,
                style: None,
                explode: false,
                allow_empty_value: false,
                allow_reserved: false,
                content_media_type: None,
                raw_schema: None,
                deprecated: false,
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "page".to_string(),
                source: ParamSource::Path,
                ty: "u32".to_string(),
                description: None,
                example: None,
                style: None,
                explode: false,
                allow_empty_value: false,
                allow_reserved: false,
                content_media_type: None,
                raw_schema: None,
                deprecated: false,
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "uuid".to_string(),
                source: ParamSource::Path,
                ty: "Uuid".to_string(),
                description: None,
                example: None,
                style: None,
                explode: false,
                allow_empty_value: false,
                allow_reserved: false,
                content_media_type: None,
                raw_schema: None,
                deprecated: false,
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "q".to_string(),
                source: ParamSource::Query,
                ty: "String".to_string(),
                description: None,
                example: None,
                style: None,
                explode: false,
                allow_empty_value: false,
                allow_reserved: false,
                content_media_type: None,
                raw_schema: None,
                deprecated: false,
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "api_key".to_string(),
                source: ParamSource::Header,
                ty: "String".to_string(),
                description: None,
                example: None,
                style: None,
                explode: false,
                allow_empty_value: false,
                allow_reserved: false,
                content_media_type: None,
                raw_schema: None,
                deprecated: false,
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "X-Custom".to_string(),
                source: ParamSource::Header,
                ty: "String".to_string(),
                description: None,
                example: None,
                style: None,
                explode: false,
                allow_empty_value: false,
                allow_reserved: false,
                content_media_type: None,
                raw_schema: None,
                deprecated: false,
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "sid".to_string(),
                source: ParamSource::Cookie,
                ty: "String".to_string(),
                description: None,
                example: None,
                style: None,
                explode: false,
                allow_empty_value: false,
                allow_reserved: false,
                content_media_type: None,
                raw_schema: None,
                deprecated: false,
                extensions: BTreeMap::new(),
            },
        ];
        r2.security = vec![
            crate::openapi::parse::models::SecurityRequirementGroup { schemes: vec![] },
            crate::openapi::parse::models::SecurityRequirementGroup {
                schemes: vec![crate::openapi::parse::models::SecurityRequirement {
                    scheme_name: "auth".to_string(),
                    scopes: vec![],
                    scheme: None,
                }],
            },
        ];
        {
            let rb = r2.request_body.as_mut().expect("request_body");
            rb.ty = "Vec<Pet>".to_string();
            rb.required = true;
        }
        let custom2 = generate_custom_test(&r2, "crate::create_app");
        assert!(custom2.contains("LoginUserQuery"));
        assert!(custom2.contains("Vec<crate::models::Pet>"));
        assert!(custom2.contains("auth_token"));

        // Test QueryString, other handler query default, multipart body, serde_json::Value body, String/Vec<u8> optional body, get_inventory
        let mut r3 = route.clone();
        r3.handler_name = "get_inventory".to_string();
        r3.params = vec![RouteParam {
            name: "raw_qs".to_string(),
            source: ParamSource::QueryString,
            ty: "String".to_string(),
            description: None,
            example: None,
            style: None,
            explode: false,
            allow_empty_value: false,
            allow_reserved: false,
            content_media_type: None,
            raw_schema: None,
            deprecated: false,
            extensions: BTreeMap::new(),
        }];
        {
            let rb = r3.request_body.as_mut().expect("request_body");
            rb.ty = "reqwest::multipart::Form".to_string();
            rb.required = true;
        }
        let custom3 = generate_custom_test(&r3, "");
        assert!(custom3.contains("reqwest::multipart::Form::new()"));
        assert!(custom3.contains("expected 200 OK and valid JSON parsing"));

        let mut r4 = route.clone();
        r4.handler_name = "other-op".to_string();
        {
            let rb = r4.request_body.as_mut().expect("request_body");
            rb.ty = "serde_json::Value".to_string();
            rb.required = true;
        }
        let custom4 = generate_custom_test(&r4, "sdk");
        assert!(custom4.contains("OtherOpQuery = Default::default()"));
        assert!(custom4.contains("serde_json::json!({})"));

        {
            let rb = r4.request_body.as_mut().expect("request_body");
            rb.ty = "Vec<u8>".to_string();
            rb.required = false;
        }
        r4.params.clear();
        let custom5 = generate_custom_test(&r4, "sdk");
        assert!(custom5.contains("let body: Option<Vec<u8>> = None;"));

        {
            let rb = r4.request_body.as_mut().expect("request_body");
            rb.ty = "String".to_string();
            rb.required = false;
        }
        let custom6 = generate_custom_test(&r4, "sdk");
        assert!(custom6.contains("let body: Option<String> = None;"));
    }
}
