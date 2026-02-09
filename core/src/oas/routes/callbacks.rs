#![deny(missing_docs)]

//! # Callback Parsing
//!
//! Logic for extracting and resolving Callback objects (Outgoing Webhooks).

use crate::error::{AppError, AppResult};
use crate::oas::models::{ParsedCallback, RuntimeExpression};
use crate::oas::resolver::extract_request_body_type;
use crate::oas::resolver::responses::extract_response_details;
use crate::oas::routes::shims::{ShimComponents, ShimOperation, ShimPathItem};
use std::collections::BTreeMap;
use utoipa::openapi::RefOr;

/// Resolves a Callback object which can be an inline map or a Reference.
pub fn resolve_callback_object(
    cb_ref: &RefOr<BTreeMap<String, ShimPathItem>>,
    components: Option<&ShimComponents>,
) -> AppResult<BTreeMap<String, ShimPathItem>> {
    match cb_ref {
        RefOr::T(map) => Ok(map.clone()),
        RefOr::Ref(r) => {
            let ref_name = r
                .ref_location
                .split('/')
                .next_back()
                .unwrap_or("Unknown")
                .to_string();

            if let Some(comps) = components {
                if let Some(cb_json) = comps.extra.get("callbacks").and_then(|c| c.get(&ref_name)) {
                    let map =
                        serde_json::from_value::<BTreeMap<String, ShimPathItem>>(cb_json.clone())
                            .map_err(|e| {
                            AppError::General(format!(
                                "Failed to parse resolved callback '{}': {}",
                                ref_name, e
                            ))
                        })?;
                    return Ok(map);
                }
            }
            Err(AppError::General(format!(
                "Callback reference not found: {}",
                r.ref_location
            )))
        }
    }
}

/// Helper to iterate methods in a Callback Path Item and extract operations.
pub fn extract_callback_operations(
    callbacks: &mut Vec<ParsedCallback>,
    name: &str,
    expression: &str,
    path_item: &ShimPathItem,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    let mut add_cb_op = |method: &str, op: &Option<ShimOperation>| -> AppResult<()> {
        if let Some(o) = op {
            let mut req_body = None;
            if let Some(rb) = &o.request_body {
                if let Some(def) = extract_request_body_type(rb)? {
                    req_body = Some(def);
                }
            }

            let (resp_type, resp_headers) =
                if let Some(details) = extract_response_details(&o.responses, components)? {
                    (details.body_type, details.headers)
                } else {
                    (None, Vec::new())
                };

            callbacks.push(ParsedCallback {
                name: name.to_string(),
                expression: RuntimeExpression::new(expression),
                method: method.to_string(),
                request_body: req_body,
                response_type: resp_type,
                response_headers: resp_headers,
            });
        }
        Ok(())
    };

    add_cb_op("GET", &path_item.get)?;
    add_cb_op("POST", &path_item.post)?;
    add_cb_op("PUT", &path_item.put)?;
    add_cb_op("DELETE", &path_item.delete)?;
    add_cb_op("PATCH", &path_item.patch)?;
    add_cb_op("OPTIONS", &path_item.options)?;
    add_cb_op("HEAD", &path_item.head)?;
    add_cb_op("TRACE", &path_item.trace)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::BodyFormat;
    use crate::oas::routes::shims::{ShimComponents, ShimOperation, ShimPathItem};
    use serde_json::json;
    use std::collections::BTreeMap;
    use utoipa::openapi::{Content, Ref, RefOr, Responses};
    use utoipa::openapi::request_body::RequestBodyBuilder;
    use utoipa::openapi::ResponseBuilder;

    fn empty_operation() -> ShimOperation {
        ShimOperation {
            operation_id: None,
            parameters: None,
            request_body: None,
            responses: Responses::new(),
            security: None,
            callbacks: None,
            tags: None,
            deprecated: false,
            external_docs: None,
            extensions: BTreeMap::new(),
        }
    }

    fn empty_path_item() -> ShimPathItem {
        ShimPathItem {
            parameters: None,
            get: None,
            post: None,
            put: None,
            delete: None,
            patch: None,
            options: None,
            head: None,
            trace: None,
            query: None,
            extensions: BTreeMap::new(),
        }
    }

    #[test]
    fn test_resolve_callback_object_inline() {
        let mut map = BTreeMap::new();
        map.insert("/hook".to_string(), empty_path_item());
        let resolved = resolve_callback_object(&RefOr::T(map.clone()), None).unwrap();
        assert_eq!(resolved.len(), 1);
        assert!(resolved.contains_key("/hook"));
    }

    #[test]
    fn test_resolve_callback_object_ref_from_components() {
        let mut components = ShimComponents {
            security_schemes: None,
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "callbacks".to_string(),
            json!({
                "OnEvent": {
                    "/event": {
                        "post": {
                            "responses": { "200": { "description": "OK" } }
                        }
                    }
                }
            }),
        );

        let cb_ref = RefOr::Ref(Ref::new("#/components/callbacks/OnEvent"));
        let resolved = resolve_callback_object(&cb_ref, Some(&components)).unwrap();
        assert!(resolved.contains_key("/event"));
    }

    #[test]
    fn test_resolve_callback_object_missing_ref() {
        let cb_ref = RefOr::Ref(Ref::new("#/components/callbacks/Missing"));
        let err = resolve_callback_object(&cb_ref, None).unwrap_err();
        assert!(format!("{}", err).contains("Callback reference not found"));
    }

    #[test]
    fn test_extract_callback_operations_collects_details() {
        let mut path_item = empty_path_item();

        let body = RequestBodyBuilder::new()
            .content(
                "application/json",
                Content::new(Some(RefOr::Ref(Ref::new(
                    "#/components/schemas/Payload",
                )))),
            )
            .build();

        let response = ResponseBuilder::new()
            .description("OK")
            .content(
                "application/json",
                Content::new(Some(RefOr::Ref(Ref::new(
                    "#/components/schemas/Ack",
                )))),
            )
            .build();

        let mut responses = Responses::new();
        responses
            .responses
            .insert("200".to_string(), RefOr::T(response));

        let mut op = empty_operation();
        op.request_body = Some(RefOr::T(body));
        op.responses = responses;
        path_item.get = Some(op);

        let mut callbacks = Vec::new();
        extract_callback_operations(
            &mut callbacks,
            "OnEvent",
            "$request.body#/callbackUrl",
            &path_item,
            None,
        )
        .unwrap();

        assert_eq!(callbacks.len(), 1);
        let cb = &callbacks[0];
        assert_eq!(cb.name, "OnEvent");
        assert_eq!(cb.method, "GET");
        assert_eq!(cb.expression.as_str(), "$request.body#/callbackUrl");
        assert_eq!(cb.request_body.as_ref().unwrap().format, BodyFormat::Json);
        assert_eq!(cb.response_type.as_deref(), Some("Ack"));
        assert!(cb.response_headers.is_empty());
    }
}
