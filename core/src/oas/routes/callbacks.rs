#![deny(missing_docs)]

//! # Callback Parsing
//!
//! Logic for extracting and resolving Callback objects (Outgoing Webhooks).

use crate::error::{AppError, AppResult};
use crate::oas::models::{ParamSource, ParsedCallback, RouteParam, RuntimeExpression};
use crate::oas::ref_utils::extract_component_name;
use crate::oas::registry::DocumentRegistry;
use crate::oas::resolver::body::extract_request_body_type_with_registry;
use crate::oas::resolver::resolve_parameters_with_registry;
use crate::oas::resolver::responses::extract_response_details_with_registry;
use crate::oas::routes::builder::parse_security_requirements;
use crate::oas::routes::shims::{ShimComponents, ShimOperation, ShimPathItem};
use std::collections::{BTreeMap, HashSet};
use url::Url;
use utoipa::openapi::RefOr;

/// Resolves a Callback object which can be an inline map or a Reference.
pub fn resolve_callback_object(
    cb_ref: &RefOr<BTreeMap<String, ShimPathItem>>,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<(
    BTreeMap<String, ShimPathItem>,
    Option<ShimComponents>,
    Option<Url>,
)> {
    match cb_ref {
        RefOr::T(map) => Ok((map.clone(), None, None)),
        RefOr::Ref(r) => {
            if let Some((comps, self_uri)) =
                components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))
            {
                if let Some(ref_name) =
                    extract_component_name(&r.ref_location, self_uri, "callbacks")
                {
                    if let Some(cb_json) =
                        comps.extra.get("callbacks").and_then(|c| c.get(&ref_name))
                    {
                        let map = serde_json::from_value::<BTreeMap<String, ShimPathItem>>(
                            cb_json.clone(),
                        )
                        .map_err(|e| {
                            AppError::General(format!(
                                "Failed to parse resolved callback '{}': {}",
                                ref_name, e
                            ))
                        })?;
                        return Ok((map, None, None));
                    }
                }
            }

            if let Some(registry) = registry {
                if let Some((raw, comps_override, base_override)) = registry
                    .resolve_component_ref_with_components(&r.ref_location, base_uri, "callbacks")
                {
                    let map = serde_json::from_value::<BTreeMap<String, ShimPathItem>>(raw)
                        .map_err(|e| {
                            AppError::General(format!(
                                "Failed to parse resolved callback '{}': {}",
                                r.ref_location, e
                            ))
                        })?;
                    return Ok((map, comps_override, base_override));
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
    operation_ids: &mut HashSet<String>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
    global_security: Option<&Vec<serde_json::Value>>,
) -> AppResult<()> {
    let mut path_item = path_item.clone();
    let mut override_components: Option<ShimComponents> = None;
    let mut override_base: Option<Url> = None;
    if let Some(ref_path) = path_item.ref_path.as_deref() {
        let empty_paths = BTreeMap::new();
        let (resolved, comps_override, base_override) = super::resolve_path_item_ref_with_context(
            ref_path,
            components,
            &empty_paths,
            registry,
            base_uri,
        )?;
        path_item = resolved;
        override_components = comps_override;
        override_base = base_override;
    }

    let components_ctx = override_components.as_ref().or(components);
    let base_ctx = override_base.as_ref().or(base_uri);

    let parsed_expression = RuntimeExpression::parse_expression(expression)?;
    let common_params_list = path_item.parameters.as_deref().unwrap_or(&[]);
    let common_params = resolve_parameters_with_registry(
        common_params_list,
        components_ctx,
        true,
        registry,
        base_ctx,
    )?;

    let mut add_cb_op = |method: &str, op: Option<&ShimOperation>| -> AppResult<()> {
        if let Some(o) = op {
            if let Some(op_id) = &o.operation_id {
                if !operation_ids.insert(op_id.clone()) {
                    return Err(AppError::General(format!(
                        "Duplicate operationId '{}' detected",
                        op_id
                    )));
                }
            }
            let op_params_list = o.parameters.as_deref().unwrap_or(&[]);
            let op_params = resolve_parameters_with_registry(
                op_params_list,
                components_ctx,
                true,
                registry,
                base_ctx,
            )?;
            let params = merge_callback_params(&op_params, &common_params, method)?;

            let mut req_body = None;
            if let Some(rb) = &o.request_body {
                if let Some(def) =
                    extract_request_body_type_with_registry(rb, components_ctx, registry, base_ctx)?
                {
                    req_body = Some(def);
                }
            }

            let security_defined = o.security.is_some();
            let security = if let Some(requirements) = o.security.as_ref() {
                if requirements.is_empty() {
                    Vec::new()
                } else {
                    parse_security_requirements(requirements, components_ctx, registry, base_ctx)
                }
            } else if let Some(global) = global_security {
                parse_security_requirements(global, components_ctx, registry, base_ctx)
            } else {
                Vec::new()
            };

            let (
                resp_type,
                resp_status,
                resp_summary,
                resp_description,
                resp_media_type,
                resp_example,
                resp_headers,
            ) = if let Some(details) = extract_response_details_with_registry(
                &o.responses,
                components_ctx,
                registry,
                base_ctx,
            )? {
                (
                    details.body_type,
                    details.status_code,
                    details.summary,
                    details.description,
                    details.media_type,
                    details.example,
                    details.headers,
                )
            } else {
                (None, None, None, None, None, None, Vec::new())
            };

            callbacks.push(ParsedCallback {
                name: name.to_string(),
                expression: parsed_expression.clone(),
                method: method.to_string(),
                params,
                path_params: common_params.clone(),
                request_body: req_body,
                response_type: resp_type,
                response_status: resp_status,
                response_summary: resp_summary,
                response_description: resp_description,
                response_media_type: resp_media_type,
                response_example: resp_example,
                response_headers: resp_headers,
                security,
                security_defined,
            });
        }
        Ok(())
    };

    add_cb_op("GET", path_item.get.as_ref())?;
    add_cb_op("POST", path_item.post.as_ref())?;
    add_cb_op("PUT", path_item.put.as_ref())?;
    add_cb_op("DELETE", path_item.delete.as_ref())?;
    add_cb_op("PATCH", path_item.patch.as_ref())?;
    add_cb_op("OPTIONS", path_item.options.as_ref())?;
    add_cb_op("HEAD", path_item.head.as_ref())?;
    add_cb_op("TRACE", path_item.trace.as_ref())?;
    add_cb_op("QUERY", path_item.query.as_ref())?;

    if let Some(additional) = &path_item.additional_operations {
        for (method, op) in additional {
            add_cb_op(method, Some(op))?;
        }
    }

    Ok(())
}

fn merge_callback_params(
    op_params: &[RouteParam],
    common_params: &[RouteParam],
    method: &str,
) -> AppResult<Vec<RouteParam>> {
    let mut params = Vec::new();
    let mut seen = HashSet::new();

    for param in op_params {
        seen.insert((param.name.clone(), param.source.clone()));
        params.push(param.clone());
    }
    for param in common_params {
        if !seen.contains(&(param.name.clone(), param.source.clone())) {
            params.push(param.clone());
        }
    }

    let querystring_count = params
        .iter()
        .filter(|p| p.source == ParamSource::QueryString)
        .count();
    if querystring_count > 1 {
        return Err(AppError::General(format!(
            "Callback operation '{}' defines multiple querystring parameters",
            method
        )));
    }
    if querystring_count == 1 && params.iter().any(|p| p.source == ParamSource::Query) {
        return Err(AppError::General(format!(
            "Callback operation '{}' mixes 'querystring' and 'query' parameters",
            method
        )));
    }

    Ok(params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::BodyFormat;
    use crate::oas::resolver::params::SchemaOrBool;
    use crate::oas::resolver::ShimParameter;
    use crate::oas::routes::shims::{ShimComponents, ShimOperation, ShimPathItem};
    use serde_json::json;
    use std::collections::BTreeMap;
    use utoipa::openapi::request_body::RequestBodyBuilder;
    use utoipa::openapi::schema::{ObjectBuilder, Schema, Type};
    use utoipa::openapi::ResponseBuilder;
    use utoipa::openapi::{Content, Ref, RefOr, Responses};

    fn empty_operation() -> ShimOperation {
        ShimOperation {
            operation_id: None,
            summary: None,
            description: None,
            parameters: None,
            request_body: None,
            responses: crate::oas::routes::shims::ShimResponses::from(Responses::new()),
            security: None,
            callbacks: None,
            tags: None,
            deprecated: false,
            external_docs: None,
            servers: None,
            extensions: BTreeMap::new(),
        }
    }

    fn empty_path_item() -> ShimPathItem {
        ShimPathItem {
            ref_path: None,
            summary: None,
            description: None,
            servers: None,
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
            additional_operations: None,
            extensions: BTreeMap::new(),
        }
    }

    fn string_param(name: &str, location: &str) -> ShimParameter {
        let schema = ObjectBuilder::new().schema_type(Type::String).build();
        ShimParameter {
            name: name.to_string(),
            description: None,
            parameter_in: location.to_string(),
            schema_type: None,
            format: None,
            items: None,
            required: false,
            deprecated: false,
            schema: Some(SchemaOrBool::from(Schema::Object(schema))),
            content: None,
            style: None,
            explode: None,
            allow_reserved: None,
            allow_empty_value: None,
            collection_format: None,
            example: None,
            examples: None,
            raw: json!({
                "name": name,
                "in": location,
                "schema": { "type": "string" }
            }),
        }
    }

    #[test]
    fn test_resolve_callback_object_inline() {
        let mut map = BTreeMap::new();
        map.insert("/hook".to_string(), empty_path_item());
        let (resolved, _, _) =
            resolve_callback_object(&RefOr::T(map.clone()), None, None, None).unwrap();
        assert_eq!(resolved.len(), 1);
        assert!(resolved.contains_key("/hook"));
    }

    #[test]
    fn test_resolve_callback_object_ref_from_components() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
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
        let (resolved, _, _) =
            resolve_callback_object(&cb_ref, Some(&components), None, None).unwrap();
        assert!(resolved.contains_key("/event"));
    }

    #[test]
    fn test_resolve_callback_object_missing_ref() {
        let cb_ref = RefOr::Ref(Ref::new("#/components/callbacks/Missing"));
        let err = resolve_callback_object(&cb_ref, None, None, None)
            .err()
            .expect("expected missing callback error");
        assert!(format!("{}", err).contains("Callback reference not found"));
    }

    #[test]
    fn test_extract_callback_operations_collects_details() {
        let mut path_item = empty_path_item();

        let body = RequestBodyBuilder::new()
            .content(
                "application/json",
                Content::new(Some(RefOr::Ref(Ref::new("#/components/schemas/Payload")))),
            )
            .build();

        let response = ResponseBuilder::new()
            .description("OK")
            .content(
                "application/json",
                Content::new(Some(RefOr::Ref(Ref::new("#/components/schemas/Ack")))),
            )
            .build();

        let mut responses = Responses::new();
        responses
            .responses
            .insert("200".to_string(), RefOr::T(response));

        let mut op = empty_operation();
        op.request_body = Some(RefOr::T(crate::oas::routes::shims::ShimRequestBody::from(
            body,
        )));
        op.responses = crate::oas::routes::shims::ShimResponses::from(responses);
        path_item.get = Some(op);

        let mut callbacks = Vec::new();
        extract_callback_operations(
            &mut callbacks,
            "OnEvent",
            "$request.body#/callbackUrl",
            &path_item,
            None,
            &mut HashSet::new(),
            None,
            None,
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

    #[test]
    fn test_extract_callback_invalid_expression_rejected() {
        let mut path_item = empty_path_item();
        path_item.get = Some(empty_operation());

        let mut callbacks = Vec::new();
        let err = extract_callback_operations(
            &mut callbacks,
            "OnEvent",
            "not-a-runtime-expression",
            &path_item,
            None,
            &mut HashSet::new(),
            None,
            None,
            None,
        )
        .unwrap_err();

        assert!(format!("{err}").contains("must include a '$' expression"));
    }

    #[test]
    fn test_extract_callback_expression_template() {
        let mut path_item = empty_path_item();
        path_item.get = Some(empty_operation());

        let mut callbacks = Vec::new();
        let expr = "https://notify.example.com?url={$request.body#/url}";
        extract_callback_operations(
            &mut callbacks,
            "OnEvent",
            expr,
            &path_item,
            None,
            &mut HashSet::new(),
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(callbacks.len(), 1);
        assert_eq!(callbacks[0].expression.as_str(), expr);
        assert!(callbacks[0].expression.is_expression());
    }

    #[test]
    fn test_extract_callback_security_inherits_global() {
        let mut path_item = empty_path_item();
        let mut op = empty_operation();
        op.responses = crate::oas::routes::shims::ShimResponses::from(Responses::new());
        path_item.post = Some(op);

        let mut callbacks = Vec::new();
        let global_security = vec![json!({ "ApiKeyAuth": [] })];

        extract_callback_operations(
            &mut callbacks,
            "OnSecure",
            "$request.body#/url",
            &path_item,
            None,
            &mut HashSet::new(),
            None,
            None,
            Some(&global_security),
        )
        .unwrap();

        assert_eq!(callbacks.len(), 1);
        let cb = &callbacks[0];
        assert!(!cb.security_defined);
        assert_eq!(cb.security.len(), 1);
        assert_eq!(cb.security[0].schemes[0].scheme_name, "ApiKeyAuth");
    }

    #[test]
    fn test_extract_callback_query_and_additional() {
        let mut path_item = empty_path_item();

        let mut op = empty_operation();
        op.responses = crate::oas::routes::shims::ShimResponses::from(Responses::new());

        path_item.query = Some(op.clone());
        let mut additional = BTreeMap::new();
        additional.insert("CUSTOM".to_string(), op);
        path_item.additional_operations = Some(additional);

        let mut callbacks = Vec::new();
        extract_callback_operations(
            &mut callbacks,
            "OnExtra",
            "$request.body#/url",
            &path_item,
            None,
            &mut HashSet::new(),
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(callbacks.len(), 2);
        let methods: Vec<_> = callbacks.iter().map(|c| c.method.as_str()).collect();
        assert!(methods.contains(&"QUERY"));
        assert!(methods.contains(&"CUSTOM"));
    }

    #[test]
    fn test_extract_callback_parameters_merge() {
        let mut path_item = empty_path_item();
        path_item.parameters = Some(vec![RefOr::T(string_param("limit", "query"))]);

        let mut op = empty_operation();
        op.parameters = Some(vec![RefOr::T(string_param("offset", "query"))]);
        path_item.post = Some(op);

        let mut callbacks = Vec::new();
        extract_callback_operations(
            &mut callbacks,
            "OnEvent",
            "$request.body#/url",
            &path_item,
            None,
            &mut HashSet::new(),
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(callbacks.len(), 1);
        let cb = &callbacks[0];
        assert_eq!(cb.path_params.len(), 1);
        assert_eq!(cb.params.len(), 2);
        assert!(cb.params.iter().any(|p| p.name == "limit"));
        assert!(cb.params.iter().any(|p| p.name == "offset"));
    }

    #[test]
    fn test_callback_path_item_ref_resolution() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            extra: BTreeMap::new(),
        };

        let mut referenced = empty_path_item();
        referenced.get = Some(empty_operation());

        let mut path_items = BTreeMap::new();
        path_items.insert("RefItem".to_string(), RefOr::T(referenced));
        components.path_items = Some(path_items);

        let mut path_item = empty_path_item();
        path_item.ref_path = Some("#/components/pathItems/RefItem".to_string());

        let mut callbacks = Vec::new();
        extract_callback_operations(
            &mut callbacks,
            "OnRef",
            "$request.body#/url",
            &path_item,
            Some(&components),
            &mut HashSet::new(),
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(callbacks.len(), 1);
        assert_eq!(callbacks[0].method, "GET");
    }
}
