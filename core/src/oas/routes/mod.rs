#![deny(missing_docs)]

//! # Routes Module
//!
//! Entry point for parsing OpenAPI `paths` and `webhooks`.
//! Orchestrates the Parsing of Shims -> Builder -> IR Models.

pub mod builder;
pub mod callbacks;
pub mod naming;
pub mod shims;

use crate::error::{AppError, AppResult};
use crate::oas::models::{ParsedRoute, RouteKind};
use crate::oas::normalization::{normalize_boolean_schemas, normalize_nullable_schemas};
use crate::oas::ref_utils::normalize_ref_to_local;
use crate::oas::registry::DocumentRegistry;
use crate::oas::routes::builder::parse_path_item;
use crate::oas::routes::shims::{ShimComponents, ShimOpenApi, ShimPathItem, ShimServer};
use crate::oas::schemas::refs::compute_base_uri;
use crate::oas::validation::{validate_component_keys, validate_openapi_root};
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};
use url::Url;
use utoipa::openapi::RefOr;

/// Parses a raw OpenAPI YAML string and extracts route definitions from `paths` and `webhooks`.
///
/// This function verifies the presence of a valid `openapi` (3.x) or `swagger` (2.0) version field
/// before processing the routes.
pub fn parse_openapi_routes(yaml_content: &str) -> AppResult<Vec<ParsedRoute>> {
    parse_openapi_routes_with_registry(yaml_content, None, None)
}

/// Parses routes with an optional document registry for external references.
pub fn parse_openapi_routes_with_registry(
    yaml_content: &str,
    registry: Option<&DocumentRegistry>,
    retrieval_uri: Option<&str>,
) -> AppResult<Vec<ParsedRoute>> {
    let mut json_val: serde_json::Value = serde_yaml::from_str(yaml_content)
        .map_err(|e| AppError::General(format!("Failed to parse OpenAPI YAML: {}", e)))?;
    parse_openapi_routes_from_value(&mut json_val, registry, retrieval_uri)
}

fn parse_openapi_routes_from_value(
    json_val: &mut serde_json::Value,
    registry: Option<&DocumentRegistry>,
    retrieval_uri: Option<&str>,
) -> AppResult<Vec<ParsedRoute>> {
    coerce_version_strings(json_val);
    normalize_link_objects(json_val);
    normalize_example_objects(json_val);
    normalize_nullable_schemas(json_val);
    normalize_boolean_schemas(json_val);
    normalize_component_refs(json_val)?;
    let openapi: ShimOpenApi = serde_json::from_value(json_val.clone())
        .map_err(|e| AppError::General(format!("Failed to parse OpenAPI YAML: {}", e)))?;

    // Version Validation
    if let Some(version) = &openapi.openapi {
        if !(version as &String).starts_with("3.") {
            return Err(AppError::General(format!(
                "Unsupported OpenAPI version: {}. Only 3.x is supported by this parser.",
                version
            )));
        }
    } else if let Some(version) = &openapi.swagger {
        if !version.starts_with("2.") {
            return Err(AppError::General(format!(
                "Unsupported Swagger version: {}. Only 2.0 is supported for legacy compatibility.",
                version
            )));
        }
    } else {
        return Err(AppError::General(
            "Invalid OpenAPI document: missing 'openapi' or 'swagger' version field.".into(),
        ));
    }

    validate_openapi_root(&openapi)?;
    if let Some(components) = openapi.components.as_ref() {
        validate_component_keys(components)?;
    }

    let base_path = resolve_base_path(&openapi, retrieval_uri);
    let base_uri = resolve_entry_base_uri(&openapi, retrieval_uri);
    let is_oas3 = openapi.openapi.is_some();
    let mut routes = Vec::new();
    let mut owned_components = openapi.components.clone();
    let empty_paths = BTreeMap::new();
    let paths = openapi
        .paths
        .as_ref()
        .map(|p| &p.items)
        .unwrap_or(&empty_paths);
    if let (Some(self_uri), Some(comps)) = (openapi.self_uri.as_deref(), owned_components.as_mut())
    {
        comps.extra.insert(
            "__self".to_string(),
            serde_json::Value::String(self_uri.to_string()),
        );
    }
    let components = owned_components.as_ref();
    let global_security = openapi.security.as_ref();
    let mut operation_index: HashMap<String, String> = HashMap::new();
    let mut operation_ids: HashSet<String> = HashSet::new();

    // 1. Parse standard Paths
    for (path_str, path_item) in paths {
        parse_path_item(
            &mut routes,
            path_str,
            path_item,
            RouteKind::Path,
            components,
            is_oas3,
            base_path.clone(),
            global_security,
            &mut operation_index,
            &mut operation_ids,
            paths,
            registry,
            base_uri.as_ref(),
        )?;
    }

    // 2. Parse Webhooks
    if let Some(webhooks) = &openapi.webhooks {
        for (name, path_item_or_ref) in &webhooks.items {
            let resolved = match path_item_or_ref {
                RefOr::T(path_item) => path_item.clone(),
                RefOr::Ref(r) => serde_json::from_value::<ShimPathItem>(serde_json::json!({
                    "$ref": r.ref_location.clone()
                }))
                .map_err(|e| {
                    AppError::General(format!(
                        "Failed to parse webhook reference '{}': {}",
                        r.ref_location, e
                    ))
                })?,
            };

            parse_path_item(
                &mut routes,
                name,
                &resolved,
                RouteKind::Webhook,
                components,
                is_oas3,
                None, // Webhooks generally don't use server prefix logic like paths
                global_security,
                &mut operation_index,
                &mut operation_ids,
                paths,
                registry,
                base_uri.as_ref(),
            )?;
        }
    }

    resolve_response_link_targets(
        &mut routes,
        &operation_index,
        openapi.self_uri.as_deref(),
        paths,
        openapi.webhooks.as_ref().map(|w| &w.items),
        openapi.components.as_ref(),
        registry,
        base_uri.as_ref(),
    )?;
    validate_link_operation_ids(&routes)?;

    Ok(routes)
}

fn resolve_entry_base_uri(openapi: &ShimOpenApi, retrieval_uri: Option<&str>) -> Option<Url> {
    let base_str = match (retrieval_uri, openapi.self_uri.as_deref()) {
        (Some(retrieval), Some(self_val)) => Some(compute_base_uri(retrieval, Some(self_val))),
        (Some(retrieval), None) => Some(retrieval.to_string()),
        (None, Some(self_val)) => Some(self_val.to_string()),
        (None, None) => None,
    }?;

    if let Ok(url) = Url::parse(&base_str) {
        return Some(url);
    }
    let dummy = Url::parse("http://example.invalid/").ok()?;
    if base_str.starts_with('/') {
        return dummy.join(&base_str).ok();
    }
    dummy.join(&base_str).ok()
}

fn resolve_response_link_targets(
    routes: &mut [ParsedRoute],
    operation_index: &HashMap<String, String>,
    self_uri: Option<&str>,
    paths: &BTreeMap<String, ShimPathItem>,
    webhooks: Option<&BTreeMap<String, RefOr<ShimPathItem>>>,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<()> {
    for route in routes.iter_mut() {
        let Some(links) = route.response_links.as_mut() else {
            continue;
        };

        for link in links.iter_mut() {
            if let Some(op_ref) = link.operation_ref.as_ref() {
                if let Some(resolved) = resolve_operation_ref_to_path(
                    op_ref, self_uri, paths, webhooks, components, registry, base_uri,
                ) {
                    link.resolved_operation_ref = Some(resolved);
                } else if is_absolute_uri(op_ref) {
                    link.resolved_operation_ref = Some(op_ref.clone());
                } else if normalize_ref_to_local(op_ref, self_uri).is_some() {
                    return Err(AppError::General(format!(
                        "Link '{}' operationRef '{}' does not resolve to a known Operation Object",
                        link.name, op_ref
                    )));
                }
                continue;
            }

            if let Some(op_id) = link.operation_id.as_ref() {
                if let Some(path) = operation_index.get(op_id) {
                    link.resolved_operation_ref = Some(path.clone());
                }
            }
        }
    }

    Ok(())
}

fn validate_link_operation_ids(routes: &[ParsedRoute]) -> AppResult<()> {
    for route in routes {
        let Some(links) = route.response_links.as_ref() else {
            continue;
        };

        for link in links {
            if link.operation_id.is_some() && link.resolved_operation_ref.is_none() {
                return Err(AppError::General(format!(
                    "Link '{}' references unknown operationId '{}'",
                    link.name,
                    link.operation_id.as_deref().unwrap_or("")
                )));
            }
        }
    }

    Ok(())
}

fn is_absolute_uri(value: &str) -> bool {
    Url::parse(value).is_ok()
}

fn resolve_operation_ref_to_path(
    operation_ref: &str,
    self_uri: Option<&str>,
    paths: &BTreeMap<String, ShimPathItem>,
    webhooks: Option<&BTreeMap<String, RefOr<ShimPathItem>>>,
    components: Option<&ShimComponents>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> Option<String> {
    if operation_ref.starts_with('/') {
        return Some(operation_ref.to_string());
    }

    let local = normalize_ref_to_local(operation_ref, self_uri)?;
    let pointer = local.trim_start_matches('#').trim_start_matches('/');
    let mut segments = pointer.split('/');
    let first = segments.next()?;
    let (key, path_item) = match first {
        "paths" | "webhooks" => {
            let path_segment = segments.next()?;
            let key = crate::oas::ref_utils::decode_pointer_segment(path_segment);
            let path_item = if first == "paths" {
                let raw = paths.get(&key)?;
                if let Some(ref_path) = raw.ref_path.as_deref() {
                    resolve_path_item_ref(ref_path, components, paths, registry, base_uri).ok()?
                } else {
                    raw.clone()
                }
            } else {
                let wh_map = webhooks?;
                let raw = wh_map.get(&key)?;
                match raw {
                    RefOr::T(item) => item.clone(),
                    RefOr::Ref(r) => resolve_path_item_ref(
                        &r.ref_location,
                        components,
                        paths,
                        registry,
                        base_uri,
                    )
                    .ok()?,
                }
            };
            (key, path_item)
        }
        "components" => {
            let section = segments.next()?;
            if section != "pathItems" {
                return None;
            }
            let name_seg = segments.next()?;
            let name = crate::oas::ref_utils::decode_pointer_segment(name_seg);
            let method_segment = segments.next();
            if segments.next().is_some() {
                return None;
            }

            let ref_str = format!("#/components/pathItems/{}", name_seg);
            let path_item =
                resolve_path_item_ref(&ref_str, components, paths, registry, base_uri).ok()?;

            if let Some(method_seg) = method_segment {
                let method = crate::oas::ref_utils::decode_pointer_segment(method_seg);
                if !path_item_supports_method(&path_item, &method) {
                    return None;
                }
            }

            let key = resolve_component_path_item_target(&name, self_uri, paths, webhooks)?;
            (key, path_item)
        }
        _ => return None,
    };

    if let Some(method_segment) = segments.next() {
        if segments.next().is_some() {
            return None;
        }
        let method = crate::oas::ref_utils::decode_pointer_segment(method_segment);
        if !path_item_supports_method(&path_item, &method) {
            return None;
        }
    }

    Some(key)
}

fn resolve_component_path_item_target(
    target_name: &str,
    self_uri: Option<&str>,
    paths: &BTreeMap<String, ShimPathItem>,
    webhooks: Option<&BTreeMap<String, RefOr<ShimPathItem>>>,
) -> Option<String> {
    let mut matches = Vec::new();

    for (path, item) in paths {
        if let Some(ref_path) = item.ref_path.as_deref() {
            if path_item_ref_matches(ref_path, self_uri, target_name) {
                matches.push(path.clone());
            }
        }
    }

    if let Some(webhooks) = webhooks {
        for (name, item_or_ref) in webhooks {
            match item_or_ref {
                RefOr::Ref(r) => {
                    if path_item_ref_matches(&r.ref_location, self_uri, target_name) {
                        matches.push(name.clone());
                    }
                }
                RefOr::T(item) => {
                    if let Some(ref_path) = item.ref_path.as_deref() {
                        if path_item_ref_matches(ref_path, self_uri, target_name) {
                            matches.push(name.clone());
                        }
                    }
                }
            }
        }
    }

    if matches.len() == 1 {
        Some(matches.remove(0))
    } else {
        None
    }
}

fn path_item_ref_matches(ref_str: &str, self_uri: Option<&str>, target_name: &str) -> bool {
    let Some(local) = normalize_ref_to_local(ref_str, self_uri) else {
        return false;
    };
    let pointer = local.trim_start_matches('#').trim_start_matches('/');
    let mut segments = pointer.split('/');
    if segments.next() != Some("components") {
        return false;
    }
    if segments.next() != Some("pathItems") {
        return false;
    }
    let name_seg = match segments.next() {
        Some(seg) => seg,
        None => return false,
    };
    if segments.next().is_some() {
        return false;
    }
    crate::oas::ref_utils::decode_pointer_segment(name_seg) == target_name
}

fn path_item_supports_method(path_item: &ShimPathItem, method: &str) -> bool {
    match method.to_ascii_lowercase().as_str() {
        "get" => path_item.get.is_some(),
        "post" => path_item.post.is_some(),
        "put" => path_item.put.is_some(),
        "delete" => path_item.delete.is_some(),
        "patch" => path_item.patch.is_some(),
        "options" => path_item.options.is_some(),
        "head" => path_item.head.is_some(),
        "trace" => path_item.trace.is_some(),
        "query" => path_item.query.is_some(),
        other => path_item
            .additional_operations
            .as_ref()
            .map(|ops| ops.keys().any(|k| k.eq_ignore_ascii_case(other)))
            .unwrap_or(false),
    }
}

fn normalize_media_type(media_type: &str) -> String {
    media_type
        .split(';')
        .next()
        .unwrap_or(media_type)
        .trim()
        .to_ascii_lowercase()
}

fn is_sequential_media_type(media_type: &str) -> bool {
    let normalized = normalize_media_type(media_type);
    matches!(
        normalized.as_str(),
        "application/jsonl"
            | "application/x-ndjson"
            | "application/json-seq"
            | "application/geo+json-seq"
            | "text/event-stream"
            | "multipart/mixed"
            | "multipart/byteranges"
    ) || normalized.ends_with("+jsonl")
        || normalized.ends_with("+ndjson")
        || normalized.ends_with("+json-seq")
}

fn normalize_link_objects(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(links_val) = map.get_mut("links") {
                normalize_link_map(links_val);
            }
            for (_, v) in map.iter_mut() {
                normalize_link_objects(v);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items.iter_mut() {
                normalize_link_objects(v);
            }
        }
        _ => {}
    }
}

fn normalize_example_objects(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if !map.contains_key("value") {
                if let Some(data_value) = map.get("dataValue").cloned() {
                    map.insert("value".to_string(), data_value);
                } else if let Some(serialized_value) = map.get("serializedValue").cloned() {
                    map.insert("value".to_string(), serialized_value);
                } else if let Some(external_value) = map.get("externalValue").cloned() {
                    map.insert("value".to_string(), external_value);
                }
            }
            for (_, v) in map.iter_mut() {
                normalize_example_objects(v);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items.iter_mut() {
                normalize_example_objects(v);
            }
        }
        _ => {}
    }
}

fn normalize_component_refs(value: &mut serde_json::Value) -> AppResult<()> {
    let self_uri = value
        .get("$self")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let is_oas3 = value.get("openapi").is_some();

    let components = value.get("components").and_then(|c| c.as_object());
    let headers = components
        .and_then(|c| c.get("headers"))
        .and_then(|h| h.as_object())
        .cloned();
    let media_types = components
        .and_then(|c| c.get("mediaTypes"))
        .and_then(|m| m.as_object())
        .cloned();

    let mut resolver = ComponentRefResolver {
        self_uri,
        headers,
        media_types,
        is_oas3,
    };
    resolver.walk(value)
}

struct ComponentRefResolver {
    self_uri: Option<String>,
    headers: Option<serde_json::Map<String, serde_json::Value>>,
    media_types: Option<serde_json::Map<String, serde_json::Value>>,
    is_oas3: bool,
}

impl ComponentRefResolver {
    fn walk(&mut self, value: &mut serde_json::Value) -> AppResult<()> {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(headers_val) = map.get_mut("headers") {
                    self.resolve_header_map(headers_val)?;
                }
                if let Some(content_val) = map.get_mut("content") {
                    self.resolve_media_type_map(content_val)?;
                }
                for (_, v) in map.iter_mut() {
                    self.walk(v)?;
                }
            }
            serde_json::Value::Array(items) => {
                for v in items.iter_mut() {
                    self.walk(v)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn resolve_header_map(&self, headers_val: &mut serde_json::Value) -> AppResult<()> {
        let serde_json::Value::Object(headers_map) = headers_val else {
            return Ok(());
        };

        let keys: Vec<String> = headers_map.keys().cloned().collect();
        for name in keys {
            if let Some(value) = headers_map.get(&name).cloned() {
                let mut header_val = value;
                if let Some(resolved) = self.resolve_header_value(&header_val)? {
                    header_val = resolved;
                }
                self.normalize_header_content_value(&name, &mut header_val)?;
                headers_map.insert(name, header_val);
            }
        }

        Ok(())
    }

    fn resolve_media_type_map(&self, content_val: &mut serde_json::Value) -> AppResult<()> {
        let serde_json::Value::Object(content_map) = content_val else {
            return Ok(());
        };

        let keys: Vec<String> = content_map.keys().cloned().collect();
        for media_type in keys {
            if let Some(value) = content_map.get(&media_type).cloned() {
                let mut media_val = value;
                if let Some(resolved) = self.resolve_media_type_value(&media_val)? {
                    media_val = resolved;
                }
                self.normalize_media_type_item_schema(&media_type, &mut media_val)?;
                content_map.insert(media_type, media_val);
            }
        }

        Ok(())
    }

    fn normalize_header_content_value(
        &self,
        header_name: &str,
        value: &mut serde_json::Value,
    ) -> AppResult<()> {
        let serde_json::Value::Object(obj) = value else {
            return Ok(());
        };

        if self.is_oas3 && obj.contains_key("example") && obj.contains_key("examples") {
            return Err(AppError::General(format!(
                "Header '{}' must not define both 'example' and 'examples'",
                header_name
            )));
        }

        if !obj.contains_key("content") {
            return Ok(());
        }

        if obj.contains_key("schema") {
            return Err(AppError::General(format!(
                "Header '{}' cannot specify both 'schema' and 'content'",
                header_name
            )));
        }

        let Some(content_val) = obj.get_mut("content") else {
            return Ok(());
        };

        self.resolve_media_type_map(content_val)?;

        let (schema_val, content_clone) = {
            let serde_json::Value::Object(content_map) = content_val else {
                return Err(AppError::General(format!(
                    "Header '{}' content must be a map of media types",
                    header_name
                )));
            };

            if content_map.len() != 1 {
                return Err(AppError::General(format!(
                    "Header '{}' content must define exactly one media type",
                    header_name
                )));
            }

            let schema_val = content_map
                .values()
                .next()
                .and_then(|media_val| {
                    if let serde_json::Value::Object(media_obj) = media_val {
                        media_obj.get("schema").cloned()
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| json!({ "type": "string" }));

            (schema_val, content_map.clone())
        };

        if !obj.contains_key(CDD_HEADER_CONTENT_KEY) {
            obj.insert(
                CDD_HEADER_CONTENT_KEY.to_string(),
                serde_json::Value::Object(content_clone),
            );
        }

        obj.insert("schema".to_string(), schema_val);
        obj.remove("content");
        Ok(())
    }

    fn normalize_media_type_item_schema(
        &self,
        media_type: &str,
        value: &mut serde_json::Value,
    ) -> AppResult<()> {
        let serde_json::Value::Object(obj) = value else {
            return Ok(());
        };

        if obj.contains_key("schema") {
            return Ok(());
        }

        let Some(item_schema) = obj.get("itemSchema").cloned() else {
            return Ok(());
        };

        if !is_sequential_media_type(media_type) {
            if self.is_oas3 {
                return Err(AppError::General(format!(
                    "Media type '{}' defines itemSchema but is not a sequential media type",
                    media_type
                )));
            }
            return Ok(());
        }

        let schema = json!({
            "type": "array",
            "items": item_schema
        });
        obj.insert("schema".to_string(), schema);
        Ok(())
    }

    fn resolve_header_value(
        &self,
        value: &serde_json::Value,
    ) -> AppResult<Option<serde_json::Value>> {
        let serde_json::Value::Object(obj) = value else {
            return Ok(None);
        };

        let Some(ref_str) = obj.get("$ref").and_then(|v| v.as_str()) else {
            return Ok(None);
        };

        let override_description = obj.get("description").cloned();
        let mut resolved =
            self.resolve_component_ref("headers", ref_str, &mut std::collections::HashSet::new())?;

        if let Some(desc) = override_description {
            if let serde_json::Value::Object(resolved_obj) = &mut resolved {
                resolved_obj.insert("description".to_string(), desc);
            }
        }

        Ok(Some(resolved))
    }

    fn resolve_media_type_value(
        &self,
        value: &serde_json::Value,
    ) -> AppResult<Option<serde_json::Value>> {
        let serde_json::Value::Object(obj) = value else {
            return Ok(None);
        };

        let Some(ref_str) = obj.get("$ref").and_then(|v| v.as_str()) else {
            return Ok(None);
        };

        let resolved = self.resolve_component_ref(
            "mediaTypes",
            ref_str,
            &mut std::collections::HashSet::new(),
        )?;
        Ok(Some(resolved))
    }

    fn resolve_component_ref(
        &self,
        section: &str,
        ref_str: &str,
        visiting: &mut std::collections::HashSet<String>,
    ) -> AppResult<serde_json::Value> {
        let self_uri = self.self_uri.as_deref();
        let name = crate::oas::ref_utils::extract_component_name(ref_str, self_uri, section)
            .ok_or_else(|| {
                AppError::General(format!("Unsupported {} $ref '{}'", section, ref_str))
            })?;
        self.resolve_component_by_name(section, &name, visiting)
    }

    fn resolve_component_by_name(
        &self,
        section: &str,
        name: &str,
        visiting: &mut std::collections::HashSet<String>,
    ) -> AppResult<serde_json::Value> {
        let key = format!("{}:{}", section, name);
        if !visiting.insert(key.clone()) {
            return Err(AppError::General(format!(
                "Component {} reference cycle detected at '{}'",
                section, name
            )));
        }

        let map = match section {
            "headers" => self.headers.as_ref(),
            "mediaTypes" => self.media_types.as_ref(),
            _ => None,
        }
        .ok_or_else(|| {
            AppError::General(format!(
                "Component section '{}' not found while resolving '{}'",
                section, name
            ))
        })?;

        let entry = map.get(name).cloned().ok_or_else(|| {
            AppError::General(format!("Component {} '{}' not found", section, name))
        })?;

        if let serde_json::Value::Object(obj) = &entry {
            if let Some(ref_str) = obj.get("$ref").and_then(|v| v.as_str()) {
                return self.resolve_component_ref(section, ref_str, visiting);
            }
        }

        Ok(entry)
    }
}

fn coerce_version_strings(value: &mut serde_json::Value) {
    let serde_json::Value::Object(map) = value else {
        return;
    };

    coerce_string_field(map, "openapi");
    coerce_string_field(map, "swagger");

    if let Some(serde_json::Value::Object(info)) = map.get_mut("info") {
        coerce_string_field(info, "version");
    }
}

fn coerce_string_field(map: &mut serde_json::Map<String, serde_json::Value>, key: &str) {
    let Some(val) = map.get_mut(key) else {
        return;
    };

    if val.is_string() {
        return;
    }

    if let Some(num) = val.as_number() {
        *val = serde_json::Value::String(num.to_string());
    } else if let Some(b) = val.as_bool() {
        *val = serde_json::Value::String(b.to_string());
    }
}

fn normalize_link_map(value: &mut serde_json::Value) {
    let serde_json::Value::Object(link_map) = value else {
        return;
    };

    for (_, link_val) in link_map.iter_mut() {
        let serde_json::Value::Object(link_obj) = link_val else {
            continue;
        };

        if !link_obj.contains_key("operation_id") {
            if let Some(op_id) = link_obj.remove("operationId") {
                link_obj.insert("operation_id".to_string(), op_id);
            }
        }
        if !link_obj.contains_key("operation_ref") {
            if let Some(op_ref) = link_obj.remove("operationRef") {
                link_obj.insert("operation_ref".to_string(), op_ref);
            }
        }
        if !link_obj.contains_key("request_body") {
            if let Some(body) = link_obj.remove("requestBody") {
                link_obj.insert("request_body".to_string(), body);
            }
        }
    }
}

const CDD_HEADER_CONTENT_KEY: &str = "x-cdd-content";

/// Resolves the base URL path from `servers` (OAS 3) or `basePath` (Swagger 2).
///
/// For servers:
/// - Takes the first server.
/// - Resolves variables with `default` values.
/// - Resolves relative URLs against the retrieval URI (when available).
/// - Extracts the path component.
fn resolve_base_path(openapi: &ShimOpenApi, retrieval_uri: Option<&str>) -> Option<String> {
    // 1. Swagger 2.0 basePath
    if let Some(bp) = &openapi.base_path {
        let trimmed = bp.trim_end_matches('/');
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    // 2. OpenAPI 3.x servers
    resolve_base_path_from_servers(openapi.servers.as_ref(), retrieval_uri)
}

/// Resolves a base path from an OAS `servers` array.
fn resolve_base_path_from_servers(
    servers: Option<&Vec<ShimServer>>,
    retrieval_uri: Option<&str>,
) -> Option<String> {
    let servers = servers?;
    let server = servers.first()?;
    let mut url = server.url.clone();

    // OAS 3.2: Resolve server variables with their default values
    if let Some(vars) = &server.variables {
        for (key, var_shim) in vars {
            let placeholder = format!("{{{}}}", key);
            url = url.replace(&placeholder, &var_shim.default);
        }
    }

    extract_base_path_from_url(&url, retrieval_uri)
}

fn extract_base_path_from_url(url: &str, retrieval_uri: Option<&str>) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(parsed) = Url::parse(trimmed) {
        return normalize_base_path(parsed.path());
    }

    // If we have a retrieval URI, resolve relative servers against it (RFC3986).
    if let Some(retrieval) = retrieval_uri {
        if let Ok(base) = Url::parse(retrieval) {
            if let Ok(resolved) = base.join(trimmed) {
                return normalize_base_path(resolved.path());
            }
        }
    }

    // Handle relative or absolute-path server URLs by resolving against a dummy base.
    if let Ok(base) = Url::parse("http://example.invalid/") {
        if let Ok(resolved) = base.join(trimmed) {
            return normalize_base_path(resolved.path());
        }
    }

    if trimmed.starts_with('/') {
        return normalize_base_path(trimmed);
    }

    normalize_base_path(&format!("/{}", trimmed))
}

fn normalize_base_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_trailing = trimmed.trim_end_matches('/');
    if without_trailing.is_empty() || without_trailing == "/" {
        return None;
    }
    if without_trailing.starts_with('/') {
        Some(without_trailing.to_string())
    } else {
        Some(format!("/{}", without_trailing))
    }
}

/// Resolves a `$ref` pointing to a Path Item from components or local paths.
pub(crate) fn resolve_path_item_ref(
    ref_str: &str,
    components: Option<&ShimComponents>,
    paths: &BTreeMap<String, ShimPathItem>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<ShimPathItem> {
    let mut visited = HashSet::new();
    resolve_path_item_ref_inner(ref_str, components, paths, registry, base_uri, &mut visited)
}

pub(crate) fn resolve_path_item_ref_with_context(
    ref_str: &str,
    components: Option<&ShimComponents>,
    paths: &BTreeMap<String, ShimPathItem>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
) -> AppResult<(ShimPathItem, Option<ShimComponents>, Option<Url>)> {
    let mut visited = HashSet::new();
    resolve_path_item_ref_with_context_inner(
        ref_str,
        components,
        paths,
        registry,
        base_uri,
        &mut visited,
    )
}

fn resolve_path_item_ref_inner(
    ref_str: &str,
    components: Option<&ShimComponents>,
    paths: &BTreeMap<String, ShimPathItem>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
    visited: &mut HashSet<String>,
) -> AppResult<ShimPathItem> {
    let mut normalized = ref_str.to_string();
    if let Some(comps) = components {
        if let Some(self_uri) = comps.extra.get("__self").and_then(|v| v.as_str()) {
            if let Some(local) = normalize_ref_to_local(ref_str, Some(self_uri)) {
                normalized = local;
            }
        }
    }

    if !visited.insert(normalized.to_string()) {
        return Err(AppError::General(format!(
            "PathItem reference cycle detected at {}",
            normalized
        )));
    }

    if !normalized.starts_with("#/") {
        if let Some(registry) = registry {
            if let Some(item) = registry.resolve_path_item_ref(&normalized, base_uri) {
                return Ok(item);
            }
        }
    }

    if let Some(pointer) = normalized.strip_prefix("#/") {
        let segments: Vec<&str> = pointer.split('/').collect();
        if segments.get(0) == Some(&"components") && segments.get(1) == Some(&"pathItems") {
            let name_seg = segments
                .get(2)
                .ok_or_else(|| AppError::General("PathItem reference missing name".into()))?;
            if segments.len() > 3 {
                return Err(AppError::General(format!(
                    "Unsupported PathItem reference depth: {}",
                    ref_str
                )));
            }
            let name = crate::oas::ref_utils::decode_pointer_segment(name_seg);
            if let Some(comps) = components {
                if let Some(map) = &comps.path_items {
                    if let Some(ref_or) = map.get(&name) {
                        match ref_or {
                            RefOr::T(pi) => return Ok(pi.clone()),
                            RefOr::Ref(r) => {
                                return resolve_path_item_ref_inner(
                                    &r.ref_location,
                                    components,
                                    paths,
                                    registry,
                                    base_uri,
                                    visited,
                                );
                            }
                        }
                    }
                }
                if let Some(path_items_val) = comps.extra.get("pathItems") {
                    if let Some(item_val) = path_items_val.get(&name) {
                        let parsed = serde_json::from_value::<ShimPathItem>(item_val.clone())
                            .map_err(|e| {
                                AppError::General(format!(
                                    "Failed to parse PathItem '{}': {}",
                                    name, e
                                ))
                            })?;
                        if let Some(next_ref) = parsed.ref_path.as_deref() {
                            return resolve_path_item_ref_inner(
                                next_ref, components, paths, registry, base_uri, visited,
                            );
                        }
                        return Ok(parsed);
                    }
                }
            }
            return Err(AppError::General(format!(
                "PathItem reference not found: {}",
                ref_str
            )));
        }

        if segments.get(0) == Some(&"paths") {
            let name_seg = segments
                .get(1)
                .ok_or_else(|| AppError::General("Path reference missing name".into()))?;
            if segments.len() > 2 {
                return Err(AppError::General(format!(
                    "Unsupported Path reference depth: {}",
                    ref_str
                )));
            }
            let path_key = crate::oas::ref_utils::decode_pointer_segment(name_seg);
            if let Some(pi) = paths.get(&path_key) {
                if let Some(next_ref) = pi.ref_path.as_deref() {
                    return resolve_path_item_ref_inner(
                        next_ref, components, paths, registry, base_uri, visited,
                    );
                }
                return Ok(pi.clone());
            }
            return Err(AppError::General(format!(
                "Path reference not found: {}",
                ref_str
            )));
        }
    }

    Err(AppError::General(format!(
        "Unsupported PathItem reference: {}",
        ref_str
    )))
}

fn resolve_path_item_ref_with_context_inner(
    ref_str: &str,
    components: Option<&ShimComponents>,
    paths: &BTreeMap<String, ShimPathItem>,
    registry: Option<&DocumentRegistry>,
    base_uri: Option<&Url>,
    visited: &mut HashSet<String>,
) -> AppResult<(ShimPathItem, Option<ShimComponents>, Option<Url>)> {
    let mut normalized = ref_str.to_string();
    if let Some(comps) = components {
        if let Some(self_uri) = comps.extra.get("__self").and_then(|v| v.as_str()) {
            if let Some(local) = normalize_ref_to_local(ref_str, Some(self_uri)) {
                normalized = local;
            }
        }
    }

    let visit_key = if let Some(base) = base_uri {
        format!("{}|{}", base, normalized)
    } else {
        normalized.clone()
    };
    if !visited.insert(visit_key) {
        return Err(AppError::General(format!(
            "PathItem reference cycle detected at {}",
            normalized
        )));
    }

    if let Some(registry) = registry {
        if let Some((item, comps_override, base_override)) =
            registry.resolve_path_item_ref_with_components(&normalized, base_uri)
        {
            let next_components = comps_override.as_ref().or(components);
            let next_base = base_override.as_ref().or(base_uri);
            if let Some(next_ref) = item.ref_path.as_deref() {
                return resolve_path_item_ref_with_context_inner(
                    next_ref,
                    next_components,
                    paths,
                    Some(registry),
                    next_base,
                    visited,
                );
            }
            return Ok((item, comps_override, base_override));
        }
    }

    if let Some(pointer) = normalized.strip_prefix("#/") {
        let segments: Vec<&str> = pointer.split('/').collect();
        if segments.get(0) == Some(&"components") && segments.get(1) == Some(&"pathItems") {
            let name_seg = segments
                .get(2)
                .ok_or_else(|| AppError::General("PathItem reference missing name".into()))?;
            if segments.len() > 3 {
                return Err(AppError::General(format!(
                    "Unsupported PathItem reference depth: {}",
                    ref_str
                )));
            }
            let name = crate::oas::ref_utils::decode_pointer_segment(name_seg);
            if let Some(comps) = components {
                if let Some(map) = &comps.path_items {
                    if let Some(ref_or) = map.get(&name) {
                        match ref_or {
                            RefOr::T(pi) => return Ok((pi.clone(), None, None)),
                            RefOr::Ref(r) => {
                                return resolve_path_item_ref_with_context_inner(
                                    &r.ref_location,
                                    components,
                                    paths,
                                    registry,
                                    base_uri,
                                    visited,
                                );
                            }
                        }
                    }
                }
                if let Some(path_items_val) = comps.extra.get("pathItems") {
                    if let Some(item_val) = path_items_val.get(&name) {
                        let parsed = serde_json::from_value::<ShimPathItem>(item_val.clone())
                            .map_err(|e| {
                                AppError::General(format!(
                                    "Failed to parse PathItem '{}': {}",
                                    name, e
                                ))
                            })?;
                        if let Some(next_ref) = parsed.ref_path.as_deref() {
                            return resolve_path_item_ref_with_context_inner(
                                next_ref, components, paths, registry, base_uri, visited,
                            );
                        }
                        return Ok((parsed, None, None));
                    }
                }
            }
            return Err(AppError::General(format!(
                "PathItem reference not found: {}",
                ref_str
            )));
        }

        if segments.get(0) == Some(&"paths") {
            let name_seg = segments
                .get(1)
                .ok_or_else(|| AppError::General("Path reference missing name".into()))?;
            if segments.len() > 2 {
                return Err(AppError::General(format!(
                    "Unsupported Path reference depth: {}",
                    ref_str
                )));
            }
            let path_key = crate::oas::ref_utils::decode_pointer_segment(name_seg);
            if let Some(pi) = paths.get(&path_key) {
                if let Some(next_ref) = pi.ref_path.as_deref() {
                    return resolve_path_item_ref_with_context_inner(
                        next_ref, components, paths, registry, base_uri, visited,
                    );
                }
                return Ok((pi.clone(), None, None));
            }
            return Err(AppError::General(format!(
                "Path reference not found: {}",
                ref_str
            )));
        }
    }

    Err(AppError::General(format!(
        "Unsupported PathItem reference: {}",
        ref_str
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::{ParamSource, RuntimeExpression, SecuritySchemeKind};
    use crate::oas::routes::shims::ShimOpenApi;
    use std::collections::BTreeMap;

    #[test]
    fn test_parse_routes_basic() {
        let yaml = r#"
openapi: 3.1.0
info: {title: T, version: 1.0}
paths:
  /users/{id}:
    parameters:
      - name: id
        in: path
        required: true
        schema: {type: string, format: uuid}
    get:
      operationId: getUserById
      responses: { '200': {description: OK} }
    post:
      operationId: UpdateUser
      requestBody:
        content:
          application/json:
            schema: { $ref: '#/components/schemas/UpdateUserRequest' }
      responses:
        '200': {description: OK, content: {application/json: {schema: {type: object}}}}
"#;
        let routes = parse_openapi_routes(yaml).unwrap();

        let get_r = routes.iter().find(|r| r.method == "GET").unwrap();
        assert_eq!(get_r.params[0].name, "id");
        assert_eq!(get_r.params[0].source, ParamSource::Path);

        let post_r = routes.iter().find(|r| r.method == "POST").unwrap();
        let body = post_r.request_body.as_ref().unwrap();
        assert_eq!(body.ty, "UpdateUserRequest");
    }

    #[test]
    fn test_parse_oas_3_2_0_compliant() {
        let yaml = r#"
openapi: 3.2.0
jsonSchemaDialect: https://spec.openapis.org/oas/3.1/dialect/base
info:
  title: OAS 3.2 Test
  version: 1.0.0
servers:
  - url: https://api.example.com/v1
    description: Production Server
paths:
  /ping:
    get:
      responses: { '200': {description: Pong} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/ping");
        assert_eq!(routes[0].base_path.as_deref(), Some("/v1"));
    }

    #[test]
    fn test_parse_response_header_content_preserved() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Headers, version: 1.0}
paths:
  /ping:
    get:
      responses:
        '200':
          description: OK
          headers:
            X-Token:
              content:
                text/plain:
                  schema: { type: string }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let header = routes[0].response_headers.first().expect("expected header");
        assert_eq!(header.name, "X-Token");
        assert_eq!(header.content_media_type.as_deref(), Some("text/plain"));
    }

    #[test]
    fn test_parse_legacy_swagger_2_0() {
        let yaml = r#"
swagger: "2.0"
info: {title: Legacy, version: 1.0}
paths:
  /legacy:
    get:
      responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/legacy");
    }

    #[test]
    fn test_server_variable_resolution() {
        let yaml = r#"
openapi: 3.2.0
info: {title: S, version: 1}
servers:
  - url: https://{env}.api.com/v1
    variables:
      env:
        default: staging
        enum: [staging, production]
paths:
  /users: # Should resolve to /v1/users in test-gen
    get:
      responses:
        '200': { description: OK }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes[0].base_path.as_deref(), Some("/v1"));
    }

    #[test]
    fn test_server_relative_url_dot() {
        let yaml = r#"
openapi: 3.2.0
info: {title: S, version: 1}
servers:
  - url: .
paths:
  /ping:
    get:
      responses:
        '200': { description: OK }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes[0].base_path.as_deref(), None);
    }

    #[test]
    fn test_server_relative_url_dot_slash() {
        let yaml = r#"
openapi: 3.2.0
info: {title: S, version: 1}
servers:
  - url: ./test
paths:
  /ping:
    get:
      responses:
        '200': { description: OK }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes[0].base_path.as_deref(), Some("/test"));
    }

    #[test]
    fn test_server_relative_url_no_leading_slash() {
        let yaml = r#"
openapi: 3.2.0
info: {title: S, version: 1}
servers:
  - url: v1
paths:
  /ping:
    get:
      responses:
        '200': { description: OK }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes[0].base_path.as_deref(), Some("/v1"));
    }

    #[test]
    fn test_server_relative_url_uses_retrieval_uri_dot() {
        let yaml = r#"
openapi: 3.2.0
info: {title: S, version: 1}
servers:
  - url: .
paths:
  /ping:
    get:
      responses:
        '200': { description: OK }
"#;
        let routes = parse_openapi_routes_with_registry(
            yaml,
            None,
            Some("https://example.com/api/openapi.yaml"),
        )
        .unwrap();
        assert_eq!(routes[0].base_path.as_deref(), Some("/api"));
    }

    #[test]
    fn test_server_relative_url_uses_retrieval_uri_dot_slash() {
        let yaml = r#"
openapi: 3.2.0
info: {title: S, version: 1}
servers:
  - url: ./v1
paths:
  /ping:
    get:
      responses:
        '200': { description: OK }
"#;
        let routes = parse_openapi_routes_with_registry(
            yaml,
            None,
            Some("https://example.com/api/openapi.yaml"),
        )
        .unwrap();
        assert_eq!(routes[0].base_path.as_deref(), Some("/api/v1"));
    }

    #[test]
    fn test_swagger_2_base_path() {
        let yaml = r#"
swagger: "2.0"
info: {title: Legacy, version: 1.0}
basePath: /api/legacy
paths:
  /old:
    get:
      responses:
        '200': { description: OK }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes[0].base_path.as_deref(), Some("/api/legacy"));
    }

    #[test]
    fn test_missing_version_fails() {
        let yaml = r#"
info: {title: Missing Version, version: 1.0}
paths: {}
"#;
        let res = parse_openapi_routes(yaml);
        assert!(res.is_err());
        match res.unwrap_err() {
            AppError::General(msg) => assert!(msg.contains("missing 'openapi' or 'swagger'")),
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_metadata_deserialization() {
        // Direct test of ShimOpenApi to verify Metadata Objects Parsing requirement
        // independent of what `parse_openapi_routes` returns.
        let yaml = r#"
openapi: 3.2.0
info:
  title: Detailed API
  description: Markdown *supported*
  termsOfService: https://example.com/terms
  contact:
    name: Support
    email: support@example.com
  license:
    name: MIT
    identifier: MIT
  version: 1.2.3
servers:
  - url: https://{env}.example.com
    variables:
      env:
        default: dev
        enum: [dev, prod]
externalDocs:
  url: https://docs.example.com
  description: Context
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(openapi.openapi.as_deref(), Some("3.2.0"));

        let info = openapi.info.unwrap();
        assert_eq!(
            info.terms_of_service.as_deref(),
            Some("https://example.com/terms")
        );

        let contact = info.contact.unwrap();
        assert_eq!(contact.email.as_deref(), Some("support@example.com"));

        let license = info.license.unwrap();
        assert_eq!(license.identifier.as_deref(), Some("MIT"));

        let servers = openapi.servers.unwrap();
        assert_eq!(servers[0].url, "https://{env}.example.com");
        let vars = servers[0].variables.as_ref().unwrap();
        assert_eq!(vars.get("env").unwrap().default, "dev");

        let ext = openapi.external_docs.unwrap();
        assert_eq!(ext.url, "https://docs.example.com");
    }

    #[test]
    fn test_route_parsing_with_reusable_params() {
        let yaml = r#"
openapi: 3.1.0
info: {title: Reuse, version: 1.0}
components:
  parameters:
    limitParam:
      name: limit
      in: query
      schema: {type: integer}
    userId:
      name: id
      in: path
      required: true
      schema: {type: string, format: uuid}
paths:
  /users/{id}:
    parameters:
      - $ref: '#/components/parameters/userId'
    get:
      parameters:
        - $ref: '#/components/parameters/limitParam'
      responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let r = &routes[0];

        assert_eq!(r.params.len(), 2);

        // userId should be parsed (common param)
        let id_p = r
            .params
            .iter()
            .find(|p| p.name == "id")
            .expect("id param missing");
        assert_eq!(id_p.source, ParamSource::Path);

        // limitParam should be parsed (Op param)
        let limit_p = r
            .params
            .iter()
            .find(|p| p.name == "limit")
            .expect("limit param missing");
        assert_eq!(limit_p.source, ParamSource::Query);
        assert!(limit_p.ty.contains("i32"));
    }

    #[test]
    fn test_nullable_param_maps_to_option() {
        let yaml = r#"
openapi: 3.0.0
info: {title: Nullable, version: 1.0}
paths:
  /items:
    get:
      parameters:
        - name: filter
          in: query
          required: true
          schema:
            type: string
            nullable: true
      responses:
        '200': { description: OK }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let param = routes[0]
            .params
            .iter()
            .find(|p| p.name == "filter")
            .expect("filter param missing");
        assert_eq!(param.ty, "Option<String>");
    }

    #[test]
    fn test_global_security_applies() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Sec, version: 1.0}
components:
  securitySchemes:
    api_key:
      type: apiKey
      name: api_key
      in: header
security:
  - api_key: []
paths:
  /secure:
    get:
      responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].security.len(), 1);
        assert_eq!(routes[0].security[0].schemes[0].scheme_name, "api_key");
    }

    #[test]
    fn test_operation_security_overrides_global() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Sec, version: 1.0}
components:
  securitySchemes:
    api_key:
      type: apiKey
      name: api_key
      in: header
    oauth:
      type: oauth2
      flows:
        implicit:
          authorizationUrl: https://auth.example.com
          scopes: { read: read }
security:
  - api_key: []
paths:
  /secure:
    get:
      security:
        - oauth: [read]
      responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].security.len(), 1);
        assert_eq!(routes[0].security[0].schemes[0].scheme_name, "oauth");
        assert_eq!(routes[0].security[0].schemes[0].scopes, vec!["read"]);
    }

    #[test]
    fn test_operation_security_empty_clears_global() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Sec, version: 1.0}
components:
  securitySchemes:
    api_key:
      type: apiKey
      name: api_key
      in: header
security:
  - api_key: []
paths:
  /public:
    get:
      security: []
      responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert!(routes[0].security.is_empty());
    }

    #[test]
    fn test_external_security_scheme_resolution_with_registry() {
        let shared = r#"
openapi: 3.2.0
$self: https://example.com/shared.yaml
info: {title: Shared, version: "1.0"}
paths: {}
components:
  securitySchemes:
    ApiKeyAuth:
      type: apiKey
      name: X-API-Key
      in: header
"#;
        let mut registry = DocumentRegistry::new();
        registry
            .register_openapi_yaml("https://example.com/shared.yaml", shared)
            .unwrap();

        let entry = r#"
openapi: 3.2.0
$self: https://example.com/openapi.yaml
info: {title: Entry, version: "1.0"}
paths:
  /secure:
    get:
      security:
        - https://example.com/shared.yaml#/components/securitySchemes/ApiKeyAuth: []
      responses: { '200': {description: OK} }
"#;

        let routes = parse_openapi_routes_with_registry(
            entry,
            Some(&registry),
            Some("https://example.com/openapi.yaml"),
        )
        .unwrap();

        assert_eq!(routes.len(), 1);
        let scheme = routes[0].security[0]
            .schemes
            .get(0)
            .and_then(|s| s.scheme.as_ref())
            .expect("security scheme missing");

        match &scheme.kind {
            SecuritySchemeKind::ApiKey { name, in_loc } => {
                assert_eq!(name, "X-API-Key");
                assert_eq!(in_loc, &ParamSource::Header);
            }
            other => panic!("expected api key scheme, got {:?}", other),
        }
    }

    #[test]
    fn test_querystring_conflict_with_query() {
        let yaml = r#"
openapi: 3.2.0
info: {title: QueryString, version: 1.0}
paths:
  /search:
    get:
      parameters:
        - name: raw
          in: querystring
          content:
            application/x-www-form-urlencoded:
              schema: { type: object }
        - name: q
          in: query
          schema: { type: string }
      responses: { '200': {description: OK} }
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("mixes 'querystring' and 'query'"));
    }

    #[test]
    fn test_querystring_duplicate_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: QueryString, version: 1.0}
paths:
  /search:
    parameters:
      - name: base
        in: querystring
        content:
          application/x-www-form-urlencoded:
            schema: { type: object }
    get:
      parameters:
        - name: override
          in: querystring
          content:
            application/x-www-form-urlencoded:
              schema: { type: object }
      responses: { '200': {description: OK} }
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("multiple querystring parameters"));
    }

    #[test]
    fn test_path_param_missing_definition() {
        let yaml = r#"
openapi: 3.2.0
info: {title: PathParam, version: 1.0}
paths:
  /users/{id}:
    get:
      responses: { '200': {description: OK} }
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("missing path parameter '{id}'"));
    }

    #[test]
    fn test_path_param_unused_definition() {
        let yaml = r#"
openapi: 3.2.0
info: {title: PathParam, version: 1.0}
paths:
  /users:
    get:
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: string }
      responses: { '200': {description: OK} }
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("is not present in path template"));
    }

    #[test]
    fn test_path_param_duplicate_in_template() {
        let yaml = r#"
openapi: 3.2.0
info: {title: PathParam, version: 1.0}
paths:
  /users/{id}/orders/{id}:
    get:
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: string }
      responses: { '200': {description: OK} }
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("contains duplicate parameter '{id}'"));
    }

    #[test]
    fn test_parse_routes_with_reusable_response() {
        let yaml = r#"
openapi: 3.1.0
info: {title: Res, version: 1}
components:
  schemas:
    MyData: { type: object }
  responses:
    SuccessResponse:
      description: S
      content:
        application/json:
          schema: { $ref: '#/components/schemas/MyData' }
paths:
  /data:
    get:
      responses:
        '200': { $ref: '#/components/responses/SuccessResponse' }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let r = &routes[0];
        assert_eq!(r.response_type.as_deref(), Some("MyData"));
    }

    #[test]
    fn test_path_item_ref_resolution() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Ref, version: 1.0}
components:
  pathItems:
    UserPath:
      get:
        responses: { '200': {description: OK} }
paths:
  /users:
    $ref: '#/components/pathItems/UserPath'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/users");
        assert_eq!(routes[0].method, "GET");
    }

    #[test]
    fn test_path_item_ref_resolution_with_self() {
        let yaml = r#"
openapi: 3.2.0
$self: https://example.com/openapi.yaml
info: {title: Ref, version: 1.0}
components:
  pathItems:
    UserPath:
      get:
        responses: { '200': {description: OK} }
paths:
  /users:
    $ref: 'https://example.com/openapi.yaml#/components/pathItems/UserPath'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "/users");
        assert_eq!(routes[0].method, "GET");
    }

    #[test]
    fn test_additional_operations_parsing() {
        let yaml = r#"
openapi: 3.2.0
info: {title: ExtraOps, version: 1.0}
paths:
  /copy:
    additionalOperations:
      COPY:
        operationId: copyThing
        responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        let r = &routes[0];
        assert_eq!(r.method, "COPY");
        assert_eq!(r.handler_name, "copy_thing");
    }

    #[test]
    fn test_additional_operations_reserved_method_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: ExtraOps, version: 1.0}
paths:
  /copy:
    additionalOperations:
      GET:
        operationId: badGet
        responses: { '200': {description: OK} }
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("additionalOperations"));
        assert!(msg.contains("GET"));
    }

    #[test]
    fn test_servers_override_precedence() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Servers, version: 1.0}
servers:
  - url: https://api.example.com/v1
paths:
  /items:
    servers:
      - url: https://api.example.com/v2
    get:
      servers:
        - url: https://api.example.com/v3
      responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].base_path.as_deref(), Some("/v3"));
        let op_servers = routes[0]
            .servers_override
            .as_ref()
            .expect("expected operation servers");
        assert_eq!(op_servers[0].url, "https://api.example.com/v3");
        let path_servers = routes[0]
            .path_servers
            .as_ref()
            .expect("expected path servers");
        assert_eq!(path_servers[0].url, "https://api.example.com/v2");
    }

    #[test]
    fn test_path_item_metadata_and_params_preserved() {
        let yaml = r#"
openapi: 3.2.0
info: {title: PathItem, version: 1.0}
paths:
  /items/{id}:
    summary: Path summary
    description: Path description
    x-path-meta:
      owner: api
    servers:
      - url: https://api.example.com/v2
    parameters:
      - name: id
        in: path
        required: true
        schema:
          type: string
    get:
      summary: Op summary
      description: Op description
      parameters:
        - name: q
          in: query
          schema:
            type: string
      responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        let route = &routes[0];
        assert_eq!(route.path_summary.as_deref(), Some("Path summary"));
        assert_eq!(route.path_description.as_deref(), Some("Path description"));
        assert_eq!(route.operation_summary.as_deref(), Some("Op summary"));
        assert_eq!(
            route.operation_description.as_deref(),
            Some("Op description")
        );
        assert!(
            route
                .path_extensions
                .get("x-path-meta")
                .and_then(|v| v.get("owner"))
                .and_then(|v| v.as_str())
                == Some("api")
        );
        assert_eq!(route.path_params.len(), 1);
        assert_eq!(route.path_params[0].name, "id");
        assert!(route.path_params[0].source == crate::oas::models::ParamSource::Path);
        assert!(
            route.path_servers.as_ref().map(|s| s[0].url.as_str())
                == Some("https://api.example.com/v2")
        );
        assert!(route
            .params
            .iter()
            .any(|p| p.name == "id" && p.source == crate::oas::models::ParamSource::Path));
        assert!(route
            .params
            .iter()
            .any(|p| p.name == "q" && p.source == crate::oas::models::ParamSource::Query));
    }

    #[test]
    fn test_webhook_ref_resolution() {
        let yaml = r#"
openapi: 3.2.0
info: {title: WebhookRef, version: 1.0}
components:
  pathItems:
    HookItem:
      post:
        responses: { '200': {description: OK} }
webhooks:
  userCreated:
    $ref: '#/components/pathItems/HookItem'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].path, "userCreated");
        assert_eq!(routes[0].method, "POST");
        assert_eq!(routes[0].kind, crate::oas::models::RouteKind::Webhook);
    }

    #[test]
    fn test_path_ref_to_paths_section() {
        let yaml = r#"
openapi: 3.2.0
info: {title: PathRef, version: 1.0}
paths:
  /base:
    get:
      responses: { '200': {description: OK} }
  /alias:
    $ref: '#/paths/~1base'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 2);
        let alias_route = routes.iter().find(|r| r.path == "/alias").unwrap();
        assert_eq!(alias_route.method, "GET");
    }

    #[test]
    fn test_paths_must_start_with_slash() {
        let yaml = r#"
openapi: 3.2.0
info: {title: BadPath, version: 1.0}
paths:
  users:
    get:
      responses: { '200': {description: OK} }
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("must start with '/'"));
    }

    #[test]
    fn test_templated_path_conflict_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: PathConflict, version: 1.0}
paths:
  /pets/{petId}:
    get:
      parameters:
        - name: petId
          in: path
          required: true
          schema: { type: string }
      responses: { '200': {description: OK} }
  /pets/{name}:
    get:
      parameters:
        - name: name
          in: path
          required: true
          schema: { type: string }
      responses: { '200': {description: OK} }
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("conflicts with"));
    }

    #[test]
    fn test_duplicate_operation_id_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: DupOp, version: 1.0}
paths:
  /a:
    get:
      operationId: sameOp
      responses: { '200': {description: OK} }
  /b:
    post:
      operationId: sameOp
      responses: { '200': {description: OK} }
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("Duplicate operationId"));
    }

    #[test]
    fn test_duplicate_operation_id_in_callback_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: DupCbOp, version: 1.0}
paths:
  /a:
    post:
      operationId: sameOp
      callbacks:
        OnEvent:
          '{$request.body#/callbackUrl}':
            post:
              operationId: sameOp
              responses: { '200': {description: OK} }
      responses: { '200': {description: OK} }
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("Duplicate operationId"));
    }

    #[test]
    fn test_operation_id_preserved_in_route() {
        let yaml = r#"
openapi: 3.2.0
info: {title: OpIdPreserve, version: 1.0}
paths:
  /users/{id}:
    parameters:
      - name: id
        in: path
        required: true
        schema: { type: string }
    get:
      operationId: GetUserById
      responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].operation_id.as_deref(), Some("GetUserById"));
        assert_eq!(routes[0].handler_name, "get_user_by_id");
    }

    #[test]
    fn test_components_key_regex_enforced() {
        let yaml = r#"
openapi: 3.2.0
info: {title: BadComponentKey, version: 1.0}
components:
  schemas:
    Bad Key!:
      type: object
paths:
  /ok:
    get:
      responses: { '200': {description: OK} }
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("Component key"));
    }

    #[test]
    fn test_openapi_requires_paths_components_or_webhooks() {
        let yaml = r#"
openapi: 3.2.0
info: {title: MissingEverything, version: 1.0}
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("must define at least one"));
    }

    #[test]
    fn test_openapi_allows_empty_paths_object() {
        let yaml = r#"
openapi: 3.2.0
info: {title: EmptyPaths, version: 1.0}
paths: {}
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert!(routes.is_empty());
    }

    #[test]
    fn test_paths_and_webhooks_extensions_are_ignored() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Extensions, version: 1.0}
paths:
  x-paths-meta: true
  /ping:
    get:
      responses: { '200': { description: OK } }
webhooks:
  x-webhooks-meta: true
  onEvent:
    post:
      responses: { '200': { description: OK } }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 2);
        assert!(routes
            .iter()
            .any(|r| r.path == "/ping" && r.kind == RouteKind::Path));
        assert!(routes
            .iter()
            .any(|r| r.path == "onEvent" && r.kind == RouteKind::Webhook));
    }

    #[test]
    fn test_path_item_ref_from_components_extra() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "pathItems".to_string(),
            serde_json::json!({
                "ExtraItem": {
                    "get": { "responses": { "200": { "description": "OK" } } }
                }
            }),
        );

        let paths = BTreeMap::new();
        let resolved = resolve_path_item_ref(
            "#/components/pathItems/ExtraItem",
            Some(&components),
            &paths,
            None,
            None,
        )
        .unwrap();
        assert!(resolved.get.is_some());
    }

    #[test]
    fn test_external_path_item_ref_with_registry() {
        let shared = r#"
openapi: 3.2.0
$self: https://example.com/shared.yaml
info: {title: Shared, version: "1.0"}
components:
  schemas:
    SharedPayload:
      type: object
  parameters:
    limitParam:
      name: limit
      in: query
      schema: { type: integer }
  requestBodies:
    SharedBody:
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/SharedPayload'
  responses:
    SharedResponse:
      description: OK
      content:
        application/json:
          schema:
            type: string
  pathItems:
    PetsPath:
      post:
        parameters:
          - $ref: '#/components/parameters/limitParam'
        requestBody:
          $ref: '#/components/requestBodies/SharedBody'
        responses:
          '200':
            $ref: '#/components/responses/SharedResponse'
"#;

        let mut registry = DocumentRegistry::new();
        registry
            .register_openapi_yaml("https://example.com/shared.yaml", shared)
            .unwrap();

        let entry = r#"
openapi: 3.2.0
$self: https://example.com/openapi.yaml
info: {title: Entry, version: "1.0"}
paths:
  /pets:
    $ref: shared.yaml#/components/pathItems/PetsPath
"#;

        let routes = parse_openapi_routes_with_registry(
            entry,
            Some(&registry),
            Some("https://example.com/openapi.yaml"),
        )
        .unwrap();

        assert_eq!(routes.len(), 1);
        let route = &routes[0];
        assert_eq!(route.path, "/pets");
        assert_eq!(route.method, "POST");
        assert!(route.params.iter().any(|p| p.name == "limit"));
        assert_eq!(
            route.request_body.as_ref().map(|b| b.ty.as_str()),
            Some("SharedPayload")
        );
        assert_eq!(route.response_type.as_deref(), Some("String"));
    }

    #[test]
    fn test_parse_callback_inline() {
        let yaml = r#"
openapi: 3.1.0
info: {title: Callback Test, version: 1.0}
paths:
  /subscribe:
    post:
      responses: { '200': {description: OK} }
      callbacks:
        onData:
          '{$request.body#/url}':
            post:
              requestBody:
                content: { application/json: { schema: {type: object} } }
              responses: { '200': {description: OK} }
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes.len(), 1);
        let route = &routes[0];

        assert_eq!(route.callbacks.len(), 1);
        let cb = &route.callbacks[0];
        assert_eq!(cb.name, "onData");
        assert_eq!(
            cb.expression,
            RuntimeExpression::new("{$request.body#/url}".to_string())
        );
        assert_eq!(cb.method, "POST");
        assert!(cb.request_body.is_some());
    }

    #[test]
    fn test_parse_callback_ref() {
        let yaml = r#"
openapi: 3.1.0
info: {title: Ref Callback, version: 1.0}
components:
  callbacks:
    MyCallback:
      '{$request.query.url}':
        put:
          responses: { '200': {description: OK} }
paths:
  /hook:
    post:
      responses: { '200': {description: OK} }
      callbacks:
        myHook:
          $ref: '#/components/callbacks/MyCallback'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let route = &routes[0];

        assert_eq!(route.callbacks.len(), 1);
        let cb = &route.callbacks[0];
        assert_eq!(cb.name, "myHook");
        assert_eq!(
            cb.expression,
            RuntimeExpression::new("{$request.query.url}")
        );
        assert_eq!(cb.method, "PUT");
    }

    #[test]
    fn test_link_operation_id_resolves_to_path() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Links, version: 1.0}
paths:
  /users/{id}:
    parameters:
      - name: id
        in: path
        required: true
        schema: { type: string }
    get:
      operationId: getUser
      responses:
        '200':
          description: ok
          links:
            Self:
              operationId: getUser
              parameters:
                id: $response.body#/id
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let links = routes[0].response_links.as_ref().unwrap();
        assert!(links[0].operation_ref.is_none());
        assert_eq!(
            links[0].resolved_operation_ref.as_deref(),
            Some("/users/{id}")
        );
    }

    #[test]
    fn test_link_operation_ref_pointer_resolves_to_path() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Links, version: 1.0}
paths:
  /users/{id}:
    parameters:
      - name: id
        in: path
        required: true
        schema: { type: string }
    get:
      operationId: getUser
      responses:
        '200':
          description: ok
          links:
            Self:
              operationRef: '#/paths/~1users~1%7Bid%7D/get'
              parameters:
                id: $response.body#/id
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let links = routes[0].response_links.as_ref().unwrap();
        assert_eq!(
            links[0].operation_ref.as_deref(),
            Some("#/paths/~1users~1%7Bid%7D/get")
        );
        assert_eq!(
            links[0].resolved_operation_ref.as_deref(),
            Some("/users/{id}")
        );
    }

    #[test]
    fn test_link_operation_ref_webhook_resolves() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Links, version: 1.0}
webhooks:
  onEvent:
    post:
      responses:
        '200': {description: ok}
paths:
  /trigger:
    post:
      responses:
        '200':
          description: ok
          links:
            Callback:
              operationRef: '#/webhooks/onEvent/post'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let links = routes[0].response_links.as_ref().unwrap();
        assert_eq!(
            links[0].operation_ref.as_deref(),
            Some("#/webhooks/onEvent/post")
        );
        assert_eq!(links[0].resolved_operation_ref.as_deref(), Some("onEvent"));
    }

    #[test]
    fn test_link_operation_ref_component_path_item_resolves() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Links, version: 1.0}
components:
  pathItems:
    UserItem:
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
      get:
        responses:
          '200':
            description: ok
            links:
              Self:
                operationRef: '#/components/pathItems/UserItem/get'
paths:
  /users/{id}:
    $ref: '#/components/pathItems/UserItem'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let links = routes[0].response_links.as_ref().unwrap();
        assert_eq!(
            links[0].operation_ref.as_deref(),
            Some("#/components/pathItems/UserItem/get")
        );
        assert_eq!(
            links[0].resolved_operation_ref.as_deref(),
            Some("/users/{id}")
        );
    }

    #[test]
    fn test_link_operation_ref_component_path_item_ambiguous_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Links, version: 1.0}
components:
  pathItems:
    UserItem:
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
      get:
        responses:
          '200':
            description: ok
            links:
              Self:
                operationRef: '#/components/pathItems/UserItem/get'
paths:
  /users/{id}:
    $ref: '#/components/pathItems/UserItem'
  /accounts/{id}:
    $ref: '#/components/pathItems/UserItem'
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("operationRef"));
    }

    #[test]
    fn test_link_operation_ref_invalid_method_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Links, version: 1.0}
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
          links:
            Self:
              operationRef: '#/paths/~1users/post'
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("operationRef"));
    }

    #[test]
    fn test_link_operation_ref_additional_operation_resolves() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Links, version: 1.0}
paths:
  /files:
    additionalOperations:
      COPY:
        responses:
          '200':
            description: ok
            links:
              Self:
                operationRef: '#/paths/~1files/COPY'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let links = routes[0].response_links.as_ref().unwrap();
        assert_eq!(
            links[0].operation_ref.as_deref(),
            Some("#/paths/~1files/COPY")
        );
        assert_eq!(links[0].resolved_operation_ref.as_deref(), Some("/files"));
    }

    #[test]
    fn test_link_operation_id_requires_target() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Links, version: 1.0}
paths:
  /users:
    get:
      responses:
        '200':
          description: ok
          links:
            Missing:
              operationId: missingOp
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("unknown operationId"));
    }

    #[test]
    fn test_response_header_ref_resolves() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Headers, version: 1.0}
components:
  headers:
    RateLimit:
      description: Requests remaining
      schema:
        type: integer
        format: int32
paths:
  /rate:
    get:
      responses:
        '200':
          description: ok
          headers:
            X-Rate-Limit:
              $ref: '#/components/headers/RateLimit'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let headers = &routes[0].response_headers;
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].name, "X-Rate-Limit");
        assert_eq!(headers[0].ty, "i32");
    }

    #[test]
    fn test_response_header_content_schema_resolves() {
        let yaml = r#"
openapi: 3.2.0
info: {title: HeaderContent, version: 1.0}
paths:
  /rate:
    get:
      responses:
        '200':
          description: ok
          headers:
            X-Rate-Limit:
              content:
                text/plain:
                  schema:
                    type: integer
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let headers = &routes[0].response_headers;
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].name, "X-Rate-Limit");
        assert_eq!(headers[0].ty, "i32");
    }

    #[test]
    fn test_response_header_content_conflict_is_error() {
        let yaml = r#"
openapi: 3.2.0
info: {title: HeaderConflict, version: 1.0}
paths:
  /rate:
    get:
      responses:
        '200':
          description: ok
          headers:
            X-Rate-Limit:
              schema:
                type: string
              content:
                text/plain:
                  schema:
                    type: integer
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("Header 'X-Rate-Limit' cannot specify both"));
    }

    #[test]
    fn test_sequential_media_type_item_schema_sets_array_type() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Sequential, version: 1.0}
components:
  schemas:
    LogEntry:
      type: object
      properties:
        id: { type: string }
paths:
  /logs:
    get:
      responses:
        '200':
          description: ok
          content:
            application/x-ndjson:
              itemSchema:
                $ref: '#/components/schemas/LogEntry'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes[0].response_type.as_deref(), Some("Vec<LogEntry>"));
    }

    #[test]
    fn test_event_stream_item_schema_sets_array_type() {
        let yaml = r#"
openapi: 3.2.0
info: {title: SSE, version: 1.0}
components:
  schemas:
    Event:
      type: object
      properties:
        data: { type: string }
paths:
  /events:
    get:
      responses:
        '200':
          description: ok
          content:
            text/event-stream:
              itemSchema:
                $ref: '#/components/schemas/Event'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes[0].response_type.as_deref(), Some("Vec<Event>"));
    }

    #[test]
    fn test_multipart_mixed_item_schema_sets_array_type() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Mixed, version: 1.0}
components:
  schemas:
    Part:
      type: object
      properties:
        id: { type: string }
paths:
  /parts:
    get:
      responses:
        '200':
          description: ok
          content:
            multipart/mixed:
              itemSchema:
                $ref: '#/components/schemas/Part'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes[0].response_type.as_deref(), Some("Vec<Part>"));
    }

    #[test]
    fn test_normalize_example_objects_data_value() {
        let mut value = serde_json::json!({
            "examples": {
                "sample": {
                    "dataValue": { "foo": "bar" }
                }
            }
        });

        normalize_example_objects(&mut value);
        assert_eq!(
            value["examples"]["sample"]["value"],
            serde_json::json!({"foo": "bar"})
        );
    }

    #[test]
    fn test_header_example_and_examples_conflict() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Headers, version: 1.0}
paths:
  /ping:
    get:
      responses:
        '200':
          description: ok
          headers:
            X-Rate:
              schema:
                type: string
              example: foo
              examples:
                ex:
                  value: bar
          content:
            application/json:
              schema:
                type: string
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("Header 'X-Rate' must not define both"));
    }

    #[test]
    fn test_item_schema_rejected_for_non_sequential_media_type() {
        let yaml = r#"
openapi: 3.2.0
info: {title: ItemSchema, version: 1.0}
paths:
  /items:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              itemSchema:
                type: string
"#;
        let err = parse_openapi_routes(yaml).unwrap_err();
        assert!(format!("{err}").contains("itemSchema but is not a sequential media type"));
    }

    #[test]
    fn test_media_type_ref_resolves_in_response() {
        let yaml = r#"
openapi: 3.2.0
info: {title: MediaTypes, version: 1.0}
components:
  schemas:
    Widget:
      type: object
      properties:
        id: { type: string }
  mediaTypes:
    WidgetJson:
      schema:
        $ref: '#/components/schemas/Widget'
paths:
  /widgets:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              $ref: '#/components/mediaTypes/WidgetJson'
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        assert_eq!(routes[0].response_type.as_deref(), Some("Widget"));
    }

    #[test]
    fn test_encoding_header_ref_resolves() {
        let yaml = r#"
openapi: 3.2.0
info: {title: EncodingHeaders, version: 1.0}
components:
  schemas:
    Upload:
      type: object
  headers:
    ContentRange:
      schema:
        type: string
paths:
  /upload:
    post:
      requestBody:
        content:
          multipart/form-data:
            schema:
              $ref: '#/components/schemas/Upload'
            encoding:
              file:
                headers:
                  Content-Range:
                    $ref: '#/components/headers/ContentRange'
      responses:
        '200': {description: ok}
"#;
        let routes = parse_openapi_routes(yaml).unwrap();
        let body = routes[0].request_body.as_ref().unwrap();
        let encoding = body.encoding.as_ref().unwrap();
        let file = encoding.get("file").unwrap();
        assert_eq!(
            file.headers.get("Content-Range").map(String::as_str),
            Some("String")
        );
    }
}
