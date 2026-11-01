#![deny(missing_docs)]

//! # Test Generation
//!
//! Generates integration and unit test scaffolding for Axum applications.

use crate::openapi::parse::{BodyFormat, ParamSource, ParsedRoute, RequestBodyDefinition};

/// Test imports.
///
/// # Returns
/// The Rust `use` statements required for Axum integration tests.
pub fn test_imports() -> String {
    let mut imports = String::new();
    imports.push_str(
        "use axum::{body::Body, http::{Request, Method, StatusCode}};
",
    );
    imports.push_str(
        "use tower::ServiceExt;
",
    );
    imports.push_str(
        "use serde_json::json;
",
    );
    imports.push_str(
        "use jsonschema::{Draft, JSONSchema};
",
    );
    imports
}

/// Test function signature.
///
/// # Arguments
/// * `fn_name` - Name of the test function (without `test_` prefix).
///
/// # Returns
/// The `#[tokio::test]` function header.
pub fn test_fn_signature(fn_name: &str) -> String {
    format!(
        "#[tokio::test]
async fn test_{}() {{",
        fn_name
    )
}

/// App initialization code.
///
/// # Arguments
/// * `app_factory` - Path to the router factory function.
///
/// # Returns
/// The Rust statement initializing `let app = ...;`.
pub fn test_app_init(app_factory: &str) -> String {
    format!(
        "    let app = {}();
",
        app_factory
    )
}

/// Body setup code supporting `Json`, `Form`, `Multipart`, `Text`, `Binary`, and `ExampleValue` payloads.
///
/// # Arguments
/// * `body` - The parsed [`RequestBodyDefinition`].
///
/// # Returns
/// Rust statements constructing `let body = Body::from(...);`.
pub fn test_body_setup_code(body: &RequestBodyDefinition) -> String {
    if let Some(example) = &body.example {
        if example.is_serialized() {
            let serialized = match example.value.as_str() {
                Some(s) => s.to_owned(),
                None => example.value.to_string(),
            };
            return format!(
                "    let body = Body::from({:?});
",
                serialized
            );
        }
        let val_str = example.value.to_string();
        return match body.format {
            BodyFormat::Json => format!(
                r#"    let req_body: serde_json::Value = serde_json::from_str({:?}).expect("valid json");
    let body = Body::from(serde_json::to_vec(&req_body).expect("serialize"));
"#,
                val_str
            ),
            BodyFormat::Form => {
                if let Some(obj) = example.value.as_object() {
                    let mut pairs_vec = Vec::new();
                    for (k, v) in obj {
                        let s = match v.as_str() {
                            Some(st) => st.to_owned(),
                            None => v.to_string(),
                        };
                        pairs_vec.push(format!("{}={}", k, s));
                    }
                    let pairs = pairs_vec.join("&");
                    format!("    let body = Body::from({:?});\n", pairs)
                } else {
                    format!("    let body = Body::from({:?});\n", val_str)
                }
            }
            BodyFormat::Multipart => {
                let payload = format!(
                    "--boundary
Content-Disposition: form-data; name='payload'

{}
--boundary--",
                    val_str
                );
                format!(
                    "    let body = Body::from({:?});
",
                    payload
                )
            }
            BodyFormat::Text => {
                let text = match example.value.as_str() {
                    Some(s) => s.to_owned(),
                    None => val_str,
                };
                format!(
                    "    let body = Body::from({:?});
",
                    text
                )
            }
            BodyFormat::Binary => "    let body = Body::from(vec![0u8, 1u8, 2u8, 3u8]);
"
            .to_string(),
        };
    }

    match body.format {
        BodyFormat::Json => {
            let mut code = String::new();
            code.push_str(r#"    let req_body = json!({ "dummy": "value" });"#);
            code.push(char::from(10u8));
            code.push_str(
                r#"    let body = Body::from(serde_json::to_vec(&req_body).expect("Failed"));"#,
            );
            code.push(char::from(10u8));
            code
        }
        BodyFormat::Form => {
            let mut s = String::from(r#"    let body = Body::from("dummy=value");"#);
            s.push(char::from(10u8));
            s
        }
        BodyFormat::Multipart => {
            let payload = "--boundary
Content-Disposition: form-data; name='field'

value
--boundary--";
            format!(
                "    let body = Body::from({:?});
",
                payload
            )
        }
        BodyFormat::Text => {
            let mut s = String::from(r#"    let body = Body::from("dummy");"#);
            s.push(char::from(10u8));
            s
        }
        BodyFormat::Binary => "    let body = Body::from(vec![0u8, 1u8, 2u8, 3u8]);
"
        .to_string(),
    }
}

/// Request builder code.
///
/// # Arguments
/// * `method` - HTTP method string.
/// * `uri` - Target request URI.
/// * `body_setup` - Optional body setup statements.
///
/// # Returns
/// Rust statements building the Axum `http::Request`.
pub fn test_request_builder(method: &str, uri: &str, body_setup: &str) -> String {
    let mut code = String::new();
    if !body_setup.is_empty() {
        code.push_str(body_setup);
    } else {
        code.push_str(
            "    let body = Body::empty();
",
        );
    }

    code.push_str(&format!(
        r#"    let request = Request::builder()
        .method(Method::{})
        .uri("{}")
        .body(body)
        .expect("Failed");
"#,
        method.to_uppercase(),
        uri
    ));
    code
}

/// API call execution.
///
/// # Returns
/// The `app.oneshot(request).await` statement.
pub fn test_api_call() -> String {
    let mut s = String::from(r#"    let response = app.oneshot(request).await.expect("Failed");"#);
    s.push(char::from(10u8));
    s
}

/// Assertion code.
///
/// # Returns
/// The status assertion statement.
pub fn test_assertion() -> String {
    "    assert_eq!(response.status(), StatusCode::OK);
"
    .to_string()
}

/// Validation helper snippet for verifying Axum HTTP responses against the OpenAPI specification.
///
/// # Returns
/// Complete Rust helper function `validate_response` for Axum `http::Response<Body>`.
pub fn test_validation_helper() -> String {
    r#"async fn validate_response(
    response: axum::http::Response<axum::body::Body>,
    method: &str,
    path_template: &str,
) {
    let status_code = response.status().as_u16();
    let yaml_content = std::fs::read_to_string(OPENAPI_PATH).expect("Failed to read OpenAPI spec");
    let openapi: serde_json::Value =
        serde_yaml::from_str(&yaml_content).expect("Failed to parse OpenAPI spec");
    let method_key = method.to_lowercase();
    if let Some(op) = openapi
        .get("paths")
        .and_then(|p| p.get(path_template))
        .and_then(|item| item.get(&method_key))
    {
        if let Some(responses) = op.get("responses").and_then(|r| r.as_object()) {
            let status_key = status_code.to_string();
            let range_key = format!("{}XX", status_code / 100);
            let matched = responses
                .get(&status_key)
                .or_else(|| responses.get(&range_key))
                .or_else(|| responses.get("default"));
            assert!(
                matched.is_some(),
                "Response status {} not documented for {} {}",
                status_code,
                method,
                path_template
            );
            if let Some(schema) = matched
                .and_then(|r| r.get("content"))
                .and_then(|c| c.get("application/json"))
                .and_then(|m| m.get("schema"))
            {
                let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .expect("Failed to read body bytes");
                let body_json: serde_json::Value =
                    serde_json::from_slice(&body_bytes).unwrap_or(serde_json::Value::Null);
                let validator = JSONSchema::options()
                    .with_draft(Draft::Draft202012)
                    .compile(schema)
                    .expect("Failed to compile JSON schema");
                if let Err(errors) = validator.validate(&body_json) {
                    let msgs: Vec<String> = errors.map(|e| e.to_string()).collect();
                    panic!("Response schema validation failed: {}", msgs.join(", "));
                }
            }
        }
    }
}
"#
    .to_string()
}

/// Resolves a sample path segment value appropriate for the parameter's Rust type.
///
/// # Arguments
/// * `route` - Input `route` parameter.
/// * `param_name` - Input `param_name` parameter.
///
/// # Returns
/// Resulting `&'static str` value.
fn sample_value_for_param(route: &ParsedRoute, param_name: &str) -> &'static str {
    let mut ty = "i64";
    for p in route.params.iter().chain(route.path_params.iter()) {
        if p.source == ParamSource::Path && p.name == param_name {
            ty = p.ty.as_str();
            break;
        }
    }

    crate::openapi::parse::models::RustType::parse(ty).sample_url_segment()
}

/// Resolves the expected HTTP status code for a route's unit test.
///
/// # Arguments
/// * `route` - Input `route` parameter.
///
/// # Returns
/// Resulting `u16` value.
fn expected_status_code(route: &ParsedRoute) -> u16 {
    let Some(status_str) = route.response_status.as_deref() else {
        return 200;
    };
    status_str
        .parse::<crate::openapi::parse::models::StatusCodeSpec>()
        .map_or(200, |spec| spec.representative_code())
}

/// Generates a unit test for a specific Axum handler function to be placed alongside it.
///
/// # Arguments
/// * `route` - The [`ParsedRoute`] under test.
///
/// # Returns
/// A complete `#[tokio::test]` function string.
pub fn handler_unit_test(route: &ParsedRoute) -> String {
    let method = match route.method.as_str() {
        "GET" => "get",
        "POST" => "post",
        "PUT" => "put",
        "DELETE" => "delete",
        "PATCH" => "patch",
        "HEAD" => "head",
        "OPTIONS" => "options",
        _ => "get",
    };

    let mut router_path = route.path.clone();
    let mut req_path = route.path.clone();
    while let Some(start) = router_path.find('{') {
        if let Some(end) = router_path[start..].find('}') {
            let actual_end = start + end;
            let param_name = router_path[start + 1..actual_end].to_string();
            let placeholder = format!("{{{}}}", param_name);
            let sample = sample_value_for_param(route, &param_name);
            router_path.replace_range(start..actual_end + 1, &format!(":{}", param_name));
            req_path = req_path.replacen(&placeholder, sample, 1);
        } else {
            break;
        }
    }

    let expected_code = expected_status_code(route);

    format!(
        r#"    #[tokio::test]
    async fn test_{handler_name}_unit() {{
        use axum::{{Router, routing::{method}}};
        use axum::http::{{Request, StatusCode}};
        use tower::ServiceExt;

        let app = Router::new().route("{router_path}", {method}(super::{handler_name}));
        let req = Request::builder()
            .method("{METHOD}")
            .uri("{req_path}")
            .body(axum::body::Body::empty())
            .expect("Failed to build request");

        let resp = app.oneshot(req).await.expect("Failed to execute request");
        assert_eq!(
            resp.status(),
            StatusCode::from_u16({expected_code}).expect("valid status")
        );
    }}"#,
        handler_name = route.handler_name,
        router_path = router_path,
        method = method,
        METHOD = route.method.to_uppercase(),
        req_path = req_path,
        expected_code = expected_code,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::{ExampleValue, RouteKind, RouteParam};
    use ra_ap_edition::Edition;
    use ra_ap_syntax::SourceFile;
    use std::collections::BTreeMap;

    fn assert_valid_rust_syntax(code: &str) {
        let parse = SourceFile::parse(code, Edition::Edition2021);
        assert!(
            parse.errors().is_empty(),
            "Generated invalid Rust AST: {:?}
Code:
{}",
            parse.errors(),
            code
        );
    }

    #[test]
    fn test_imports_and_validation_helper_ast() {
        let full = format!(
            "{}
{}
{}",
            test_imports(),
            r#"const OPENAPI_PATH: &str = "openapi.yaml";"#,
            test_validation_helper()
        );
        assert_valid_rust_syntax(&full);
    }

    #[test]
    fn test_body_setup_all_formats_ast() {
        let formats = [
            (
                BodyFormat::Json,
                Some(ExampleValue::data(serde_json::json!({"a": 1}))),
            ),
            (
                BodyFormat::Json,
                Some(ExampleValue::serialized(serde_json::json!("raw_json"))),
            ),
            (
                BodyFormat::Json,
                Some(ExampleValue::serialized(serde_json::json!(123))),
            ),
            (BodyFormat::Json, None),
            (
                BodyFormat::Form,
                Some(ExampleValue::data(serde_json::json!({"k": "v", "num": 42}))),
            ),
            (
                BodyFormat::Form,
                Some(ExampleValue::data(serde_json::json!("raw_form"))),
            ),
            (BodyFormat::Form, None),
            (
                BodyFormat::Multipart,
                Some(ExampleValue::data(serde_json::json!({"f": "v"}))),
            ),
            (BodyFormat::Multipart, None),
            (
                BodyFormat::Text,
                Some(ExampleValue::data(serde_json::json!("hello"))),
            ),
            (
                BodyFormat::Text,
                Some(ExampleValue::data(serde_json::json!(99))),
            ),
            (BodyFormat::Text, None),
            (
                BodyFormat::Binary,
                Some(ExampleValue::data(serde_json::json!("bytes"))),
            ),
            (BodyFormat::Binary, None),
        ];

        for (format, example) in formats {
            let def = RequestBodyDefinition {
                ty: "Payload".to_string(),
                description: None,
                media_type: "application/json".to_string(),
                format,
                required: true,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
                example,
            };
            let setup = test_body_setup_code(&def);
            let wrapped = format!(
                "{}
{}
{}{}{}{}
}}",
                test_imports(),
                test_fn_signature("sample"),
                test_app_init("create_app"),
                test_request_builder("POST", "/items", &setup),
                test_api_call(),
                test_assertion()
            );
            assert_valid_rust_syntax(&wrapped);
        }
    }

    #[test]
    fn test_handler_unit_test_with_uuid_and_status() {
        let route = ParsedRoute {
            method: "POST".into(),
            handler_name: "my_handler".into(),
            path: "/users/{id}/flags/{active}".into(),
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
            operation_id: None,
            params: vec![
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
                    name: "id".to_string(),
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
                    name: "active".to_string(),
                    source: ParamSource::Path,
                    ty: "bool".to_string(),
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
            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: Some("201".to_string()),
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            tags: vec![],
            extensions: BTreeMap::new(),
        };

        let generated = handler_unit_test(&route);
        assert_valid_rust_syntax(&generated);
        assert!(generated.contains("test_my_handler_unit"));
        assert!(generated.contains("post(super::my_handler)"));
        assert!(generated.contains("/users/00000000-0000-0000-0000-000000000000/flags/true"));
        assert!(generated.contains("StatusCode::from_u16(201)"));

        let empty_req = test_request_builder("GET", "/users", "");
        assert!(empty_req.contains("Body::empty()"));

        for m in ["GET", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "CUSTOM"] {
            let mut r = route.clone();
            r.method = m.to_string();
            r.path = "/items/{missing_param}/{unclosed".to_string();
            r.response_status = None;
            let gen = handler_unit_test(&r);
            assert!(gen.contains(":missing_param/{unclosed"));
            assert!(gen.contains("StatusCode::from_u16(200)"));
        }
    }
}
