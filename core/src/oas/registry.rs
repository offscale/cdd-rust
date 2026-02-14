#![deny(missing_docs)]

//! # Document Registry
//!
//! Stores externally supplied OpenAPI / JSON Schema documents for multi-document
//! reference resolution. No network access is performed.

use crate::error::{AppError, AppResult};
use crate::oas::normalization::{
    normalize_boolean_schemas, normalize_const_schemas, normalize_nullable_schemas,
};
use crate::oas::ref_utils::decode_pointer_segment;
use crate::oas::routes::shims::{ShimComponents, ShimOpenApi, ShimPathItem};
use crate::oas::schemas::refs::{
    collect_inline_schema_index, collect_schema_anchors, collect_schema_ids, compute_base_uri,
    parse_reference,
};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use url::Url;
use utoipa::openapi::{Components, OpenApi, RefOr, Schema};

const DUMMY_BASE: &str = "http://example.invalid/";

/// The kind of document stored in the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    /// A full OpenAPI document with `openapi` or `swagger`.
    OpenApi,
    /// A standalone JSON Schema document.
    Schema,
}

/// A target schema referenced by `$id` or `$anchor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaTarget {
    /// A schema rooted at the document itself.
    Root(usize),
    /// A component schema within an OpenAPI document.
    Component {
        /// Index of the document entry in the registry.
        doc: usize,
        /// Component schema name within the OpenAPI document.
        name: String,
    },
}

/// A single registered document entry.
pub struct DocumentEntry {
    /// Retrieval URI used when registering the document.
    pub retrieval_uri: String,
    /// Base URI resolved from `$self` / retrieval URI.
    pub base_uri: Option<Url>,
    /// Document kind.
    pub kind: DocumentKind,
    /// Raw JSON representation of the document.
    pub raw: JsonValue,
    /// Parsed OpenAPI shim (only for OpenAPI documents).
    pub shim: Option<ShimOpenApi>,
    /// Parsed components (only for OpenAPI documents).
    pub components: Option<Components>,
    /// Parsed root schema (only for schema documents).
    pub schema_root: Option<Schema>,
}

/// Registry for externally supplied OpenAPI / JSON Schema documents.
#[derive(Default)]
pub struct DocumentRegistry {
    docs: Vec<DocumentEntry>,
    index: HashMap<String, usize>,
    schema_ids: HashMap<String, SchemaTarget>,
    schema_anchors: HashMap<String, SchemaTarget>,
    inline_schema_ids: HashMap<String, Schema>,
    inline_schema_anchors: HashMap<String, Schema>,
}

impl DocumentRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an OpenAPI document from YAML.
    pub fn register_openapi_yaml(&mut self, retrieval_uri: &str, yaml: &str) -> AppResult<()> {
        let raw: JsonValue = serde_yaml::from_str(yaml)
            .map_err(|e| AppError::General(format!("Failed to parse OpenAPI YAML: {}", e)))?;
        self.register_openapi_json(retrieval_uri, raw)
    }

    /// Registers a JSON Schema document from YAML.
    pub fn register_schema_yaml(&mut self, retrieval_uri: &str, yaml: &str) -> AppResult<()> {
        let raw: JsonValue = serde_yaml::from_str(yaml)
            .map_err(|e| AppError::General(format!("Failed to parse Schema YAML: {}", e)))?;
        self.register_schema_json(retrieval_uri, raw)
    }

    /// Registers an OpenAPI document from JSON value.
    pub fn register_openapi_json(&mut self, retrieval_uri: &str, raw: JsonValue) -> AppResult<()> {
        let mut shim: ShimOpenApi = serde_json::from_value(raw.clone()).map_err(|e| {
            AppError::General(format!(
                "Failed to parse OpenAPI shim for '{}': {}",
                retrieval_uri, e
            ))
        })?;
        if let (Some(self_uri), Some(comps)) = (shim.self_uri.as_deref(), shim.components.as_mut())
        {
            comps.extra.insert(
                "__self".to_string(),
                serde_json::Value::String(self_uri.to_string()),
            );
        }

        let base_uri = resolve_base_url(Some(retrieval_uri), shim.self_uri.as_deref());

        let components_model = parse_components_model(&raw)?;

        let entry = DocumentEntry {
            retrieval_uri: retrieval_uri.to_string(),
            base_uri,
            kind: DocumentKind::OpenApi,
            raw: raw.clone(),
            shim: Some(shim),
            components: components_model,
            schema_root: None,
        };

        let idx = self.insert_entry(entry)?;
        self.register_schema_indices_for_openapi(idx)?;
        let inline_index =
            collect_inline_schema_index(&raw, self.docs[idx].base_uri.as_ref(), true);
        self.inline_schema_ids.extend(inline_index.ids);
        self.inline_schema_anchors.extend(inline_index.anchors);

        Ok(())
    }

    /// Registers a JSON Schema document from JSON value.
    pub fn register_schema_json(
        &mut self,
        retrieval_uri: &str,
        mut raw: JsonValue,
    ) -> AppResult<()> {
        normalize_nullable_schemas(&mut raw);
        normalize_boolean_schemas(&mut raw);
        normalize_const_schemas(&mut raw);

        let schema_root: Schema = serde_json::from_value(raw.clone()).map_err(|e| {
            AppError::General(format!(
                "Failed to parse Schema document '{}': {}",
                retrieval_uri, e
            ))
        })?;

        let base_uri = resolve_base_url(Some(retrieval_uri), None);

        let entry = DocumentEntry {
            retrieval_uri: retrieval_uri.to_string(),
            base_uri,
            kind: DocumentKind::Schema,
            raw: raw.clone(),
            shim: None,
            components: None,
            schema_root: Some(schema_root),
        };

        let idx = self.insert_entry(entry)?;
        self.register_schema_indices_for_root(idx)?;
        let inline_index =
            collect_inline_schema_index(&raw, self.docs[idx].base_uri.as_ref(), false);
        self.inline_schema_ids.extend(inline_index.ids);
        self.inline_schema_anchors.extend(inline_index.anchors);

        Ok(())
    }

    /// Returns a registered document by any known URI.
    pub fn get(&self, uri: &str) -> Option<&DocumentEntry> {
        self.index.get(uri).and_then(|idx| self.docs.get(*idx))
    }

    /// Resolves an external schema `$ref` against the registry.
    pub fn resolve_schema_ref<'a>(
        &'a self,
        ref_str: &str,
        base: Option<&Url>,
    ) -> Option<&'a Schema> {
        let parsed = parse_reference(ref_str);

        if let Some(target) = self.resolve_schema_id(&parsed, base) {
            if let Some(schema) = self.schema_from_target(target) {
                return Some(schema);
            }
        }

        if let Some(schema) = self.resolve_inline_schema_id(&parsed, base) {
            return Some(schema);
        }

        if let Some(target) = self.resolve_schema_anchor(&parsed, base) {
            if let Some(schema) = self.schema_from_target(target) {
                return Some(schema);
            }
        }

        if let Some(schema) = self.resolve_inline_schema_anchor(&parsed, base) {
            return Some(schema);
        }

        if parsed.document.is_empty() {
            return None;
        }

        let doc_uri = resolve_doc_uri(parsed.document, base)?;
        let entry = self.get(&doc_uri)?;

        match entry.kind {
            DocumentKind::Schema => {
                if fragment_is_root(parsed.fragment) {
                    return entry.schema_root.as_ref();
                }
                None
            }
            DocumentKind::OpenApi => {
                let frag = parsed.fragment?;
                if !frag.starts_with('/') {
                    return None;
                }
                let segments: Vec<&str> = frag.trim_start_matches('/').split('/').collect();
                if segments.get(0) != Some(&"components") || segments.get(1) != Some(&"schemas") {
                    return None;
                }
                let name_seg = segments.get(2)?;
                let name = decode_pointer_segment(name_seg);
                let comps = entry.components.as_ref()?;
                match comps.schemas.get(&name) {
                    Some(RefOr::T(schema)) => Some(schema),
                    _ => None,
                }
            }
        }
    }

    /// Resolves an external component `$ref` (non-schema) to its raw JSON value.
    pub fn resolve_component_ref(
        &self,
        ref_str: &str,
        base: Option<&Url>,
        section: &str,
    ) -> Option<JsonValue> {
        let parsed = parse_reference(ref_str);
        let doc_uri = if parsed.document.is_empty() {
            base.map(|b| b.to_string())?
        } else {
            resolve_doc_uri(parsed.document, base)?
        };
        let entry = self.get(&doc_uri)?;
        let fragment = parsed.fragment?;
        if !fragment.starts_with('/') {
            return None;
        }
        let segments: Vec<&str> = fragment.trim_start_matches('/').split('/').collect();
        if segments.get(0) != Some(&"components") || segments.get(1) != Some(&section) {
            return None;
        }
        let name_seg = segments.get(2)?;
        let name = decode_pointer_segment(name_seg);
        entry.component_value(section, &name)
    }

    /// Resolves a Path Item `$ref` across external OpenAPI documents.
    pub fn resolve_path_item_ref(&self, ref_str: &str, base: Option<&Url>) -> Option<ShimPathItem> {
        let parsed = parse_reference(ref_str);
        let doc_uri = if parsed.document.is_empty() {
            base.map(|b| b.to_string())?
        } else {
            resolve_doc_uri(parsed.document, base)?
        };
        let entry = self.get(&doc_uri)?;
        if entry.kind != DocumentKind::OpenApi {
            return None;
        }
        let fragment = parsed.fragment?;
        if !fragment.starts_with('/') {
            return None;
        }
        let segments: Vec<&str> = fragment.trim_start_matches('/').split('/').collect();
        if segments.get(0) == Some(&"components") && segments.get(1) == Some(&"pathItems") {
            let name_seg = segments.get(2)?;
            let name = decode_pointer_segment(name_seg);
            if let Some(shim) = entry.shim.as_ref() {
                if let Some(comps) = shim.components.as_ref() {
                    if let Some(map) = &comps.path_items {
                        if let Some(ref_or) = map.get(&name) {
                            return match ref_or {
                                RefOr::T(pi) => Some(pi.clone()),
                                RefOr::Ref(_) => None,
                            };
                        }
                    }
                    if let Some(value) = comps.extra.get("pathItems").and_then(|m| m.get(&name)) {
                        return serde_json::from_value::<ShimPathItem>(value.clone()).ok();
                    }
                }
            }
        }

        if segments.get(0) == Some(&"paths") {
            let name_seg = segments.get(1)?;
            let path_key = decode_pointer_segment(name_seg);
            if let Some(shim) = entry.shim.as_ref() {
                if let Some(paths) = shim.paths.as_ref() {
                    if let Some(item) = paths.items.get(&path_key) {
                        return Some(item.clone());
                    }
                }
            }
        }

        None
    }

    pub(crate) fn resolve_component_ref_with_components(
        &self,
        ref_str: &str,
        base: Option<&Url>,
        section: &str,
    ) -> Option<(JsonValue, Option<ShimComponents>, Option<Url>)> {
        let parsed = parse_reference(ref_str);
        let doc_uri = if parsed.document.is_empty() {
            base.map(|b| b.to_string())?
        } else {
            resolve_doc_uri(parsed.document, base)?
        };
        let entry = self.get(&doc_uri)?;
        let raw = self.resolve_component_ref(ref_str, base, section)?;
        let components = entry.shim.as_ref().and_then(|shim| shim.components.clone());
        let base_uri = entry.base_uri.clone();
        Some((raw, components, base_uri))
    }

    pub(crate) fn resolve_path_item_ref_with_components(
        &self,
        ref_str: &str,
        base: Option<&Url>,
    ) -> Option<(ShimPathItem, Option<ShimComponents>, Option<Url>)> {
        let parsed = parse_reference(ref_str);
        let doc_uri = if parsed.document.is_empty() {
            base.map(|b| b.to_string())?
        } else {
            resolve_doc_uri(parsed.document, base)?
        };
        let entry = self.get(&doc_uri)?;
        if entry.kind != DocumentKind::OpenApi {
            return None;
        }
        let fragment = parsed.fragment?;
        if !fragment.starts_with('/') {
            return None;
        }
        let segments: Vec<&str> = fragment.trim_start_matches('/').split('/').collect();
        let mut resolved = None;

        if segments.get(0) == Some(&"components") && segments.get(1) == Some(&"pathItems") {
            let name_seg = segments.get(2)?;
            let name = decode_pointer_segment(name_seg);
            if let Some(shim) = entry.shim.as_ref() {
                if let Some(comps) = shim.components.as_ref() {
                    if let Some(map) = &comps.path_items {
                        if let Some(ref_or) = map.get(&name) {
                            resolved = match ref_or {
                                RefOr::T(pi) => Some(pi.clone()),
                                RefOr::Ref(_) => serde_json::to_value(ref_or)
                                    .ok()
                                    .and_then(|v| serde_json::from_value::<ShimPathItem>(v).ok()),
                            };
                        }
                    }
                    if resolved.is_none() {
                        if let Some(value) = comps.extra.get("pathItems").and_then(|m| m.get(&name))
                        {
                            resolved = serde_json::from_value::<ShimPathItem>(value.clone()).ok();
                        }
                    }
                }
            }
        }

        if resolved.is_none() && segments.get(0) == Some(&"paths") {
            let name_seg = segments.get(1)?;
            let path_key = decode_pointer_segment(name_seg);
            if let Some(shim) = entry.shim.as_ref() {
                if let Some(paths) = shim.paths.as_ref() {
                    if let Some(item) = paths.items.get(&path_key) {
                        resolved = Some(item.clone());
                    }
                }
            }
        }

        let resolved = resolved?;
        let components = entry.shim.as_ref().and_then(|shim| shim.components.clone());
        let base_uri = entry.base_uri.clone();
        Some((resolved, components, base_uri))
    }

    fn insert_entry(&mut self, entry: DocumentEntry) -> AppResult<usize> {
        let idx = self.docs.len();
        let mut aliases = Vec::new();

        aliases.push(entry.retrieval_uri.clone());
        if let Some(base) = &entry.base_uri {
            aliases.push(base.to_string());
        }

        for alias in &aliases {
            if let Some(existing) = self.index.get(alias) {
                return Err(AppError::General(format!(
                    "Document registry URI collision for '{}': already registered as {}",
                    alias, self.docs[*existing].retrieval_uri
                )));
            }
        }

        self.docs.push(entry);
        for alias in aliases {
            self.index.insert(alias, idx);
        }

        Ok(idx)
    }

    fn register_schema_indices_for_openapi(&mut self, doc_idx: usize) -> AppResult<()> {
        let Some(shim) = self.docs[doc_idx].shim.as_ref() else {
            return Ok(());
        };
        let Some(comps) = shim.components.as_ref() else {
            return Ok(());
        };
        let raw_schemas = comps.extra.get("schemas").and_then(|v| v.as_object());
        let base_uri = self.docs[doc_idx].base_uri.as_ref();

        let ids = collect_schema_ids(raw_schemas, base_uri);
        for (uri, name) in ids {
            self.schema_ids
                .insert(uri, SchemaTarget::Component { doc: doc_idx, name });
        }

        let anchors = collect_schema_anchors(raw_schemas, base_uri);
        for (anchor, name) in anchors {
            self.schema_anchors
                .insert(anchor, SchemaTarget::Component { doc: doc_idx, name });
        }

        Ok(())
    }

    fn register_schema_indices_for_root(&mut self, doc_idx: usize) -> AppResult<()> {
        let entry = &self.docs[doc_idx];
        let Some(obj) = entry.raw.as_object() else {
            return Ok(());
        };

        let base_uri = entry.base_uri.as_ref();
        if let Some(id) = obj.get("$id").and_then(|v| v.as_str()) {
            let resolved = resolve_doc_uri(id, base_uri);
            if let Some(uri) = resolved {
                self.schema_ids.insert(uri, SchemaTarget::Root(doc_idx));
            }
        }

        let base = obj
            .get("$id")
            .and_then(|v| v.as_str())
            .and_then(|id| resolve_doc_uri(id, base_uri))
            .or_else(|| entry.base_uri.as_ref().map(|u| u.to_string()));

        if let Some(anchor) = obj.get("$anchor").and_then(|v| v.as_str()) {
            insert_root_anchor(&mut self.schema_anchors, anchor, base.as_deref(), doc_idx);
        }
        if let Some(anchor) = obj.get("$dynamicAnchor").and_then(|v| v.as_str()) {
            insert_root_anchor(&mut self.schema_anchors, anchor, base.as_deref(), doc_idx);
        }

        Ok(())
    }

    fn resolve_schema_id<'a>(
        &'a self,
        parsed: &crate::oas::schemas::refs::ParsedReference<'_>,
        base: Option<&Url>,
    ) -> Option<&'a SchemaTarget> {
        if parsed.document.is_empty() {
            return None;
        }
        if let Some(frag) = parsed.fragment {
            if !frag.is_empty() && frag != "/" {
                return None;
            }
        }
        let doc_uri = resolve_doc_uri(parsed.document, base)?;
        self.schema_ids.get(&doc_uri)
    }

    fn resolve_schema_anchor<'a>(
        &'a self,
        parsed: &crate::oas::schemas::refs::ParsedReference<'_>,
        base: Option<&Url>,
    ) -> Option<&'a SchemaTarget> {
        let frag = parsed.fragment?;
        if frag.is_empty() || frag.starts_with('/') {
            return None;
        }

        let mut candidates = Vec::new();
        candidates.push(format!("#{}", frag));

        if !parsed.document.is_empty() {
            if let Some(doc_uri) = resolve_doc_uri(parsed.document, base) {
                candidates.push(format!("{}#{}", doc_uri, frag));
            }
        } else if let Some(base) = base {
            candidates.push(format!("{}#{}", base, frag));
        }

        for candidate in candidates {
            if let Some(target) = self.schema_anchors.get(&candidate) {
                return Some(target);
            }
        }
        None
    }

    fn resolve_inline_schema_id<'a>(
        &'a self,
        parsed: &crate::oas::schemas::refs::ParsedReference<'_>,
        base: Option<&Url>,
    ) -> Option<&'a Schema> {
        if parsed.document.is_empty() {
            return None;
        }
        if let Some(frag) = parsed.fragment {
            if !frag.is_empty() && frag != "/" {
                return None;
            }
        }
        let doc_uri = resolve_doc_uri(parsed.document, base)?;
        self.inline_schema_ids.get(&doc_uri)
    }

    fn resolve_inline_schema_anchor<'a>(
        &'a self,
        parsed: &crate::oas::schemas::refs::ParsedReference<'_>,
        base: Option<&Url>,
    ) -> Option<&'a Schema> {
        let frag = parsed.fragment?;
        if frag.is_empty() || frag.starts_with('/') {
            return None;
        }

        let mut candidates = Vec::new();
        candidates.push(format!("#{}", frag));

        if !parsed.document.is_empty() {
            if let Some(doc_uri) = resolve_doc_uri(parsed.document, base) {
                candidates.push(format!("{}#{}", doc_uri, frag));
            }
        } else if let Some(base) = base {
            candidates.push(format!("{}#{}", base, frag));
        }

        for candidate in candidates {
            if let Some(schema) = self.inline_schema_anchors.get(&candidate) {
                return Some(schema);
            }
        }

        None
    }

    fn schema_from_target<'a>(&'a self, target: &SchemaTarget) -> Option<&'a Schema> {
        match target {
            SchemaTarget::Root(doc) => self.docs.get(*doc)?.schema_root.as_ref(),
            SchemaTarget::Component { doc, name } => {
                let entry = self.docs.get(*doc)?;
                let comps = entry.components.as_ref()?;
                match comps.schemas.get(name) {
                    Some(RefOr::T(schema)) => Some(schema),
                    _ => None,
                }
            }
        }
    }
}

impl DocumentEntry {
    fn component_value(&self, section: &str, name: &str) -> Option<JsonValue> {
        let Some(shim) = &self.shim else {
            return None;
        };
        let Some(comps) = shim.components.as_ref() else {
            return None;
        };

        if section == "pathItems" {
            if let Some(map) = &comps.path_items {
                if let Some(ref_or) = map.get(name) {
                    return serde_json::to_value(ref_or).ok();
                }
            }
        }

        if section == "securitySchemes" {
            if let Some(map) = &comps.security_schemes {
                if let Some(ref_or) = map.get(name) {
                    return serde_json::to_value(ref_or).ok();
                }
            }
        }

        comps
            .extra
            .get(section)
            .and_then(|section_map| section_map.get(name))
            .cloned()
    }
}

fn resolve_base_url(retrieval_uri: Option<&str>, self_uri: Option<&str>) -> Option<Url> {
    let base_str = match (retrieval_uri, self_uri) {
        (Some(retrieval), Some(self_val)) => compute_base_uri(retrieval, Some(self_val)),
        (Some(retrieval), None) => retrieval.to_string(),
        (None, Some(self_val)) => self_val.to_string(),
        (None, None) => return None,
    };

    parse_base_url(&base_str)
}

fn parse_base_url(base_str: &str) -> Option<Url> {
    if let Ok(url) = Url::parse(base_str) {
        return Some(url);
    }
    let dummy = Url::parse(DUMMY_BASE).ok()?;
    if base_str.starts_with('/') {
        return dummy.join(base_str).ok();
    }
    dummy.join(base_str).ok()
}

fn resolve_doc_uri(doc: &str, base: Option<&Url>) -> Option<String> {
    if let Ok(url) = Url::parse(doc) {
        return Some(url.to_string());
    }
    let base = base?;
    base.join(doc).ok().map(|u| u.to_string())
}

fn fragment_is_root(fragment: Option<&str>) -> bool {
    match fragment {
        None => true,
        Some(frag) => frag.is_empty() || frag == "/",
    }
}

fn insert_root_anchor(
    anchors: &mut HashMap<String, SchemaTarget>,
    anchor: &str,
    base: Option<&str>,
    doc_idx: usize,
) {
    let trimmed = anchor.trim();
    if trimmed.is_empty() {
        return;
    }
    anchors.insert(format!("#{}", trimmed), SchemaTarget::Root(doc_idx));
    if let Some(base_uri) = base {
        anchors.insert(
            format!("{}#{}", base_uri, trimmed),
            SchemaTarget::Root(doc_idx),
        );
    }
}

fn parse_components_model(raw: &JsonValue) -> AppResult<Option<Components>> {
    let mut normalized = raw.clone();
    normalize_nullable_schemas(&mut normalized);
    normalize_boolean_schemas(&mut normalized);
    normalize_const_schemas(&mut normalized);

    if normalized.get("paths").is_none() {
        if let Some(obj) = normalized.as_object_mut() {
            obj.insert(
                "paths".to_string(),
                JsonValue::Object(serde_json::Map::new()),
            );
        }
    }

    if let Some(ver) = normalized.get_mut("openapi") {
        if let Some(raw) = ver.as_str() {
            if raw.starts_with("3.") && !raw.starts_with("3.1") {
                *ver = serde_json::json!("3.1.0");
            }
        }
    }

    let openapi: OpenApi = serde_json::from_value(normalized)
        .map_err(|e| AppError::General(format!("Failed to parse OpenAPI components: {}", e)))?;

    Ok(openapi.components)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_resolves_schema_id_and_anchor() {
        let schema = r#"
$id: https://example.com/schemas/base.json
$anchor: Base
type: object
properties:
  id:
    type: string
"#;

        let mut registry = DocumentRegistry::new();
        registry
            .register_schema_yaml("https://example.com/schemas/base.json", schema)
            .unwrap();

        let ref_by_id = "https://example.com/schemas/base.json";
        assert!(registry.resolve_schema_ref(ref_by_id, None).is_some());

        let ref_by_anchor = "https://example.com/schemas/base.json#Base";
        assert!(registry.resolve_schema_ref(ref_by_anchor, None).is_some());
    }

    #[test]
    fn test_registry_resolves_inline_schema_id_and_anchor_in_openapi() {
        let openapi = r#"
openapi: 3.2.0
info:
  title: Inline
  version: "1.0"
paths:
  /foo:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                $id: https://example.com/schemas/Inline
                $anchor: InlineAnchor
                type: string
"#;

        let mut registry = DocumentRegistry::new();
        registry
            .register_openapi_yaml("https://example.com/openapi.yaml", openapi)
            .unwrap();

        let ref_by_id = "https://example.com/schemas/Inline";
        assert!(registry.resolve_schema_ref(ref_by_id, None).is_some());

        let ref_by_anchor = "#InlineAnchor";
        assert!(registry.resolve_schema_ref(ref_by_anchor, None).is_some());
    }
}
