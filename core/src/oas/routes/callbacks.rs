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
