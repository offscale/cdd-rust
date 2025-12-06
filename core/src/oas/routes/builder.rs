#![deny(missing_docs)]

//! # Route Builder
//!
//! Logic that transforms `Shim` structs into `ParsedRoute` IR models.
//! This includes resolving parameters, request bodies, and security requirements.

use crate::error::AppResult;
use crate::oas::models::{ParsedRoute, RouteKind, RouteParam, SecurityRequirement};
use crate::oas::resolver::{
    extract_request_body_type, extract_response_success_type, resolve_parameters,
};
use crate::oas::routes::callbacks::{extract_callback_operations, resolve_callback_object};
use crate::oas::routes::naming::{derive_handler_name, to_snake_case};
use crate::oas::routes::shims::{ShimOperation, ShimPathItem};
use serde_json::Value;
use std::collections::HashMap;

/// Helper to iterate methods in a ShimPathItem and extract all operations as Routes.
///
/// # Arguments
///
/// * `routes` - Accumulator for parsed routes.
/// * `path_or_name` - The URL path or webhook name.
/// * `path_item` - The generic path item struct.
/// * `kind` - Differentiator for Webhook vs Path.
/// * `components` - Reference storage.
pub fn parse_path_item(
    routes: &mut Vec<ParsedRoute>,
    path_or_name: &str,
    path_item: ShimPathItem,
    kind: RouteKind,
    components: Option<&Value>,
) -> AppResult<()> {
    // Handle common parameters defined at PathItem level.
    let common_params_list = path_item.parameters.as_deref().unwrap_or(&[]);
    let common_params = resolve_parameters(common_params_list, components)?;

    let mut add_op = |method: &str, op: Option<ShimOperation>| -> AppResult<()> {
        if let Some(o) = op {
            routes.push(build_route(
                path_or_name,
                method,
                o,
                &common_params,
                kind.clone(),
                components,
            )?);
        }
        Ok(())
    };

    add_op("GET", path_item.get)?;
    add_op("POST", path_item.post)?;
    add_op("PUT", path_item.put)?;
    add_op("DELETE", path_item.delete)?;
    add_op("PATCH", path_item.patch)?;
    add_op("OPTIONS", path_item.options)?;
    add_op("HEAD", path_item.head)?;
    add_op("TRACE", path_item.trace)?;

    Ok(())
}

fn build_route(
    path: &str,
    method: &str,
    op: ShimOperation,
    common_params: &[RouteParam],
    kind: RouteKind,
    components: Option<&Value>,
) -> AppResult<ParsedRoute> {
    // 1. Handler Name
    let handler_name = if let Some(op_id) = &op.operation_id {
        to_snake_case(op_id)
    } else {
        derive_handler_name(method, path)
    };

    // 2. Parameters
    let op_params_list = op.parameters.as_deref().unwrap_or(&[]);
    let op_params = resolve_parameters(op_params_list, components)?;

    let mut params = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Operation params take precedence over Common params
    for p in op_params {
        seen.insert((p.name.clone(), p.source.clone()));
        params.push(p);
    }
    for p in common_params {
        if !seen.contains(&(p.name.clone(), p.source.clone())) {
            params.push(p.clone());
        }
    }

    // 3. Request Body
    let mut request_body = None;
    if let Some(req_body_ref) = &op.request_body {
        if let Some(def) = extract_request_body_type(req_body_ref)? {
            request_body = Some(def);
        }
    }

    // 4. Security
    let mut security = Vec::new();
    if let Some(requirements) = &op.security {
        for req in requirements {
            if let Ok(map) = serde_json::from_value::<HashMap<String, Vec<String>>>(req.clone()) {
                for (scheme, scopes) in map {
                    security.push(SecurityRequirement {
                        scheme_name: scheme,
                        scopes,
                    });
                }
            }
        }
    }

    // 5. Response Type
    let response_type = extract_response_success_type(&op.responses, components)?;

    // 6. Callbacks
    let mut parsed_callbacks = Vec::new();
    if let Some(cb_map) = &op.callbacks {
        for (cb_name, cb_ref) in cb_map {
            // Resolve RefOr -> BTreeMap<Expression, PathItem>
            let callback_defs = resolve_callback_object(cb_ref, components)?;

            for (expression, path_item) in callback_defs {
                // Flatten the PathItem inside the callback into ParsedCallback entries
                extract_callback_operations(
                    &mut parsed_callbacks,
                    cb_name,
                    &expression,
                    &path_item,
                    components,
                )?;
            }
        }
    }

    Ok(ParsedRoute {
        path: path.to_string(),
        method: method.to_string(),
        handler_name,
        params,
        request_body,
        security,
        response_type,
        kind,
        callbacks: parsed_callbacks,
    })
}
