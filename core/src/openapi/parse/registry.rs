#![deny(missing_docs)]

//! # Document Registry
//!
//! Stores externally supplied OpenAPI / JSON Schema documents for multi-document
//! reference resolution. No network access is performed.

use crate::error::{AppError, AppResult};
use crate::openapi::parse::normalization::{
    normalize_boolean_schemas, normalize_const_schemas, normalize_nullable_schemas,
};
use crate::openapi::parse::ref_utils::decode_pointer_segment;
use crate::openapi::parse::routes::shims::{ShimComponents, ShimOpenApi, ShimPathItem};
use crate::openapi::parse::schemas::refs::{
    collect_inline_schema_index, collect_schema_anchors, collect_schema_ids, compute_base_uri,
    parse_reference,
};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use url::Url;
use utoipa::openapi::{Components, RefOr, Schema};

/// Constant `DUMMY_BASE` (dummy base).
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
    /// Field `docs` (docs).
    docs: Vec<DocumentEntry>,
    /// Field `index` (index).
    index: HashMap<String, usize>,
    /// Field `schema_ids` (schema ids).
    schema_ids: HashMap<String, SchemaTarget>,
    /// Field `schema_anchors` (schema anchors).
    schema_anchors: HashMap<String, SchemaTarget>,
    /// Field `inline_schema_ids` (inline schema ids).
    inline_schema_ids: HashMap<String, Schema>,
    /// Field `inline_schema_anchors` (inline schema anchors).
    inline_schema_anchors: HashMap<String, Schema>,
}

impl DocumentRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an OpenAPI document from YAML.
    ///
    /// # Arguments
    /// * `retrieval_uri` - Input `retrieval_uri` parameter.
    /// * `yaml` - Input `yaml` parameter.
    ///
    /// # Returns
    /// Resulting `AppResult<()>` value.
    ///
    /// # Errors
    /// Returns an error if validation, parsing, or execution fails.
    pub fn register_openapi_yaml(&mut self, retrieval_uri: &str, yaml: &str) -> AppResult<()> {
        let raw: JsonValue = serde_yaml::from_str(yaml)
            .map_err(|e| AppError::General(format!("Failed to parse OpenAPI YAML: {}", e)))?;
        self.register_openapi_json(retrieval_uri, raw)
    }

    /// Registers a JSON Schema document from YAML.
    ///
    /// # Arguments
    /// * `retrieval_uri` - Input `retrieval_uri` parameter.
    /// * `yaml` - Input `yaml` parameter.
    ///
    /// # Returns
    /// Resulting `AppResult<()>` value.
    ///
    /// # Errors
    /// Returns an error if validation, parsing, or execution fails.
    pub fn register_schema_yaml(&mut self, retrieval_uri: &str, yaml: &str) -> AppResult<()> {
        let raw: JsonValue = serde_yaml::from_str(yaml)
            .map_err(|e| AppError::General(format!("Failed to parse Schema YAML: {}", e)))?;
        self.register_schema_json(retrieval_uri, raw)
    }

    /// Registers an OpenAPI document from JSON value.
    ///
    /// # Arguments
    /// * `retrieval_uri` - Input `retrieval_uri` parameter.
    /// * `raw` - Input `raw` parameter.
    ///
    /// # Returns
    /// Resulting `AppResult<()>` value.
    ///
    /// # Errors
    /// Returns an error if validation, parsing, or execution fails.
    pub fn register_openapi_json(&mut self, retrieval_uri: &str, raw: JsonValue) -> AppResult<()> {
        let mut shim: ShimOpenApi = serde_json::from_value(raw.clone()).map_err(|e| {
            AppError::General(format!(
                "Failed to parse OpenAPI shim for '{}': {}",
                retrieval_uri, e
            ))
        })?;
        if let (Some(self_uri), Some(comps)) = (shim.self_uri.as_deref(), shim.components.as_mut())
        {
            comps.self_uri = Some(self_uri.to_string());
        }

        let base_uri = resolve_base_url(retrieval_uri, shim.self_uri.as_deref());

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
        self.register_schema_indices_for_openapi(idx);
        let inline_index =
            collect_inline_schema_index(&raw, self.docs[idx].base_uri.as_ref(), true);
        self.inline_schema_ids.extend(inline_index.ids);
        self.inline_schema_anchors.extend(inline_index.anchors);

        Ok(())
    }

    /// Registers a JSON Schema document from JSON value.
    ///
    /// # Arguments
    /// * `retrieval_uri` - Input `retrieval_uri` parameter.
    /// * `raw` - Input `raw` parameter.
    ///
    /// # Returns
    /// Resulting `AppResult<()>` value.
    ///
    /// # Errors
    /// Returns an error if validation, parsing, or execution fails.
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

        let base_uri = resolve_base_url(retrieval_uri, None);

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
        self.register_schema_indices_for_root(idx);
        let inline_index =
            collect_inline_schema_index(&raw, self.docs[idx].base_uri.as_ref(), false);
        self.inline_schema_ids.extend(inline_index.ids);
        self.inline_schema_anchors.extend(inline_index.anchors);

        Ok(())
    }

    /// Returns a registered document by any known URI.
    ///
    /// # Arguments
    /// * `uri` - Input `uri` parameter.
    ///
    /// # Returns
    /// Resulting `Option<&DocumentEntry>` value.
    pub fn get(&self, uri: &str) -> Option<&DocumentEntry> {
        self.index.get(uri).and_then(|idx| self.docs.get(*idx))
    }

    /// Resolves an external schema `$ref` against the registry.
    ///
    /// # Arguments
    /// * `ref_str` - Input `ref_str` parameter.
    /// * `base` - Input `base` parameter.
    ///
    /// # Returns
    /// Resulting `Option<&'a Schema>` value.
    pub fn resolve_schema_ref<'a>(
        &'a self,
        ref_str: &str,
        base: Option<&Url>,
    ) -> Option<&'a Schema> {
        let parsed = parse_reference(ref_str);

        if let Some(target) = self.resolve_schema_id(&parsed, base) {
            return self.schema_from_target(target);
        }

        if let Some(schema) = self.resolve_inline_schema_id(&parsed, base) {
            return Some(schema);
        }

        if let Some(target) = self.resolve_schema_anchor(&parsed, base) {
            return self.schema_from_target(target);
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
                if segments.first() != Some(&"components") || segments.get(1) != Some(&"schemas") {
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
    ///
    /// # Arguments
    /// * `ref_str` - Input `ref_str` parameter.
    /// * `base` - Input `base` parameter.
    /// * `section` - Input `section` parameter.
    ///
    /// # Returns
    /// Resulting `Option<JsonValue>` value.
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
        if segments.first() != Some(&"components") || segments.get(1) != Some(&section) {
            return None;
        }
        let name_seg = segments.get(2)?;
        let name = decode_pointer_segment(name_seg);
        entry.component_value(section, &name)
    }

    /// Resolves a Path Item `$ref` across external OpenAPI documents.
    ///
    /// # Arguments
    /// * `ref_str` - Input `ref_str` parameter.
    /// * `base` - Input `base` parameter.
    ///
    /// # Returns
    /// Resulting `Option<ShimPathItem>` value.
    pub fn resolve_path_item_ref(&self, ref_str: &str, base: Option<&Url>) -> Option<ShimPathItem> {
        self.resolve_path_item_ref_with_components(ref_str, base)
            .map(|(pi, _, _)| pi)
    }

    /// Executes `resolve_component_ref_with_components` (resolve component ref with components).
    ///
    /// # Arguments
    /// * `ref_str` - Input `ref_str` parameter.
    /// * `base` - Input `base` parameter.
    /// * `section` - Input `section` parameter.
    ///
    /// # Returns
    /// Resulting `Option<(JsonValue, Option<ShimComponents>, Option<Url>)>` value.
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

    /// Executes `resolve_path_item_ref_with_components` (resolve path item ref with components).
    ///
    /// # Arguments
    /// * `ref_str` - Input `ref_str` parameter.
    /// * `base` - Input `base` parameter.
    ///
    /// # Returns
    /// Resulting `Option<(ShimPathItem, Option<ShimComponents>, Option<Url>)>` value.
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

        if segments.first() == Some(&"components") && segments.get(1) == Some(&"pathItems") {
            let name_seg = segments.get(2)?;
            let name = decode_pointer_segment(name_seg);
            let ref_or_opt = entry
                .shim
                .as_ref()
                .and_then(|s| s.components.as_ref())
                .and_then(|c| c.path_items.as_ref())
                .and_then(|m| m.get(&name));
            if let Some(ref_or) = ref_or_opt {
                resolved = match ref_or {
                    RefOr::T(pi) => Some(pi.clone()),
                    RefOr::Ref(_) => serde_json::to_value(ref_or)
                        .ok()
                        .and_then(|v| serde_json::from_value::<ShimPathItem>(v).ok()),
                };
            }
        }

        if resolved.is_none() && segments.first() == Some(&"paths") {
            let name_seg = segments.get(1)?;
            let path_key = decode_pointer_segment(name_seg);
            resolved = entry
                .shim
                .as_ref()
                .and_then(|s| s.paths.as_ref())
                .and_then(|p| p.items.get(&path_key))
                .cloned();
        }

        let resolved = resolved?;
        let components = entry.shim.as_ref().and_then(|shim| shim.components.clone());
        let base_uri = entry.base_uri.clone();
        Some((resolved, components, base_uri))
    }

    /// Executes `insert_entry` (insert entry).
    ///
    /// # Arguments
    /// * `entry` - Input `entry` parameter.
    ///
    /// # Returns
    /// Resulting `AppResult<usize>` value.
    ///
    /// # Errors
    /// Returns an error if validation, parsing, or processing fails.
    fn insert_entry(&mut self, entry: DocumentEntry) -> AppResult<usize> {
        let idx = self.docs.len();
        let mut aliases = Vec::new();

        aliases.push(entry.retrieval_uri.clone());
        if let Some(base) = &entry.base_uri {
            let base_str = base.to_string();
            if base_str != entry.retrieval_uri {
                aliases.push(base_str);
            }
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

    /// Executes `register_schema_indices_for_openapi` (register schema indices for openapi).
    ///
    /// # Arguments
    /// * `doc_idx` - Input `doc_idx` parameter.
    fn register_schema_indices_for_openapi(&mut self, doc_idx: usize) {
        let Some(comps) = self.docs[doc_idx]
            .shim
            .as_ref()
            .and_then(|s| s.components.as_ref())
        else {
            return;
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
    }

    /// Executes `register_schema_indices_for_root` (register schema indices for root).
    ///
    /// # Arguments
    /// * `doc_idx` - Input `doc_idx` parameter.
    fn register_schema_indices_for_root(&mut self, doc_idx: usize) {
        let entry = &self.docs[doc_idx];
        let obj = &entry.raw;

        let base_uri = entry.base_uri.as_ref();
        if let Some(id) = obj.get("$id").and_then(|v| v.as_str()) {
            if let Some(uri) = resolve_doc_uri(id, base_uri) {
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
    }

    /// Executes `resolve_schema_id` (resolve schema id).
    ///
    /// # Arguments
    /// * `parsed` - Input `parsed` parameter.
    /// * `base` - Input `base` parameter.
    ///
    /// # Returns
    /// Resulting `Option<&'a SchemaTarget>` value.
    fn resolve_schema_id<'a>(
        &'a self,
        parsed: &crate::openapi::parse::schemas::refs::ParsedReference<'_>,
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

    /// Executes `resolve_schema_anchor` (resolve schema anchor).
    ///
    /// # Arguments
    /// * `parsed` - Input `parsed` parameter.
    /// * `base` - Input `base` parameter.
    ///
    /// # Returns
    /// Resulting `Option<&'a SchemaTarget>` value.
    fn resolve_schema_anchor<'a>(
        &'a self,
        parsed: &crate::openapi::parse::schemas::refs::ParsedReference<'_>,
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

        candidates
            .iter()
            .find_map(|candidate| self.schema_anchors.get(candidate))
    }

    /// Executes `resolve_inline_schema_id` (resolve inline schema id).
    ///
    /// # Arguments
    /// * `parsed` - Input `parsed` parameter.
    /// * `base` - Input `base` parameter.
    ///
    /// # Returns
    /// Resulting `Option<&'a Schema>` value.
    fn resolve_inline_schema_id<'a>(
        &'a self,
        parsed: &crate::openapi::parse::schemas::refs::ParsedReference<'_>,
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

    /// Executes `resolve_inline_schema_anchor` (resolve inline schema anchor).
    ///
    /// # Arguments
    /// * `parsed` - Input `parsed` parameter.
    /// * `base` - Input `base` parameter.
    ///
    /// # Returns
    /// Resulting `Option<&'a Schema>` value.
    fn resolve_inline_schema_anchor<'a>(
        &'a self,
        parsed: &crate::openapi::parse::schemas::refs::ParsedReference<'_>,
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

        candidates
            .iter()
            .find_map(|candidate| self.inline_schema_anchors.get(candidate))
    }

    /// Executes `schema_from_target` (schema from target).
    ///
    /// # Arguments
    /// * `target` - Input `target` parameter.
    ///
    /// # Returns
    /// Resulting `Option<&Schema>` value.
    fn schema_from_target(&self, target: &SchemaTarget) -> Option<&Schema> {
        match target {
            SchemaTarget::Root(doc) => self.docs[*doc].schema_root.as_ref(),
            SchemaTarget::Component { doc, name } => {
                let comps = self.docs[*doc].components.as_ref()?;
                let schema_ref = comps.schemas.get(name)?;
                extract_concrete_schema(schema_ref)
            }
        }
    }
}

/// Executes `extract_concrete_schema` (extract concrete schema).
///
/// # Arguments
/// * `ref_or` - Input `ref_or` parameter.
///
/// # Returns
/// Resulting `Option<&Schema>` value.
fn extract_concrete_schema(ref_or: &RefOr<Schema>) -> Option<&Schema> {
    match ref_or {
        RefOr::T(schema) => Some(schema),
        RefOr::Ref(_) => None,
    }
}

impl DocumentEntry {
    /// Executes `component_value` (component value).
    ///
    /// # Arguments
    /// * `section` - Input `section` parameter.
    /// * `name` - Input `name` parameter.
    ///
    /// # Returns
    /// Resulting `Option<JsonValue>` value.
    fn component_value(&self, section: &str, name: &str) -> Option<JsonValue> {
        let comps = self.shim.as_ref()?.components.as_ref()?;

        if section == "pathItems" {
            if let Some(ref_or) = comps.path_items.as_ref().and_then(|m| m.get(name)) {
                return serde_json::to_value(ref_or).ok();
            }
        }

        if section == "securitySchemes" {
            if let Some(ref_or) = comps.security_schemes.as_ref().and_then(|m| m.get(name)) {
                return serde_json::to_value(ref_or).ok();
            }
        }

        comps
            .extra
            .get(section)
            .and_then(|section_map| section_map.get(name))
            .cloned()
    }
}

/// Executes `resolve_base_url` (resolve base url).
///
/// # Arguments
/// * `retrieval_uri` - Input `retrieval_uri` parameter.
/// * `self_uri` - Input `self_uri` parameter.
///
/// # Returns
/// Resulting `Option<Url>` value.
fn resolve_base_url(retrieval_uri: &str, self_uri: Option<&str>) -> Option<Url> {
    let base_str = compute_base_uri(retrieval_uri, self_uri);
    parse_base_url(&base_str)
}

/// Executes `parse_base_url` (parse base url).
///
/// # Arguments
/// * `base_str` - Input `base_str` parameter.
///
/// # Returns
/// Resulting `Option<Url>` value.
fn parse_base_url(base_str: &str) -> Option<Url> {
    if let Ok(url) = Url::parse(base_str) {
        return Some(url);
    }
    Url::parse(DUMMY_BASE)
        .ok()
        .and_then(|dummy| dummy.join(base_str).ok())
}

/// Executes `resolve_doc_uri` (resolve doc uri).
///
/// # Arguments
/// * `doc` - Input `doc` parameter.
/// * `base` - Input `base` parameter.
///
/// # Returns
/// Resulting `Option<String>` value.
fn resolve_doc_uri(doc: &str, base: Option<&Url>) -> Option<String> {
    if let Ok(url) = Url::parse(doc) {
        return Some(url.to_string());
    }
    let base = base?;
    base.join(doc).ok().map(|u| u.to_string())
}

/// Executes `fragment_is_root` (fragment is root).
///
/// # Arguments
/// * `fragment` - Input `fragment` parameter.
///
/// # Returns
/// Resulting `bool` value.
fn fragment_is_root(fragment: Option<&str>) -> bool {
    match fragment {
        None => true,
        Some(frag) => frag.is_empty() || frag == "/",
    }
}

/// Executes `insert_root_anchor` (insert root anchor).
///
/// # Arguments
/// * `anchors` - Input `anchors` parameter.
/// * `anchor` - Input `anchor` parameter.
/// * `base` - Input `base` parameter.
/// * `doc_idx` - Input `doc_idx` parameter.
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

/// Executes `parse_components_model` (parse components model).
///
/// # Arguments
/// * `raw` - Input `raw` parameter.
///
/// # Returns
/// Resulting `AppResult<Option<Components>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn parse_components_model(raw: &JsonValue) -> AppResult<Option<Components>> {
    let mut normalized = raw.clone();
    normalize_nullable_schemas(&mut normalized);
    normalize_boolean_schemas(&mut normalized);
    normalize_const_schemas(&mut normalized);

    let Some(raw_comps) = normalized.get("components").and_then(|c| c.as_object()) else {
        return Ok(None);
    };

    let mut parsed_components = Components::new();
    if let Some(schema_map) = raw_comps.get("schemas").and_then(|s| s.as_object()) {
        for (name, val) in schema_map {
            let schema_ref: RefOr<Schema> = serde_json::from_value(val.clone()).map_err(|e| {
                AppError::General(format!("Failed to parse OpenAPI components: {}", e))
            })?;
            parsed_components.schemas.insert(name.clone(), schema_ref);
        }
    }

    Ok(Some(parsed_components))
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
            .expect("Failed to register schema yaml");

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
            .expect("Failed to register openapi yaml");

        let ref_by_id = "https://example.com/schemas/Inline";
        assert!(registry.resolve_schema_ref(ref_by_id, None).is_some());

        let ref_by_anchor = "#InlineAnchor";
        assert!(registry.resolve_schema_ref(ref_by_anchor, None).is_some());

        // Cover error paths, component_value, resolve_path_item_ref, resolve_component_ref,
        // SchemaTarget::Component, $dynamicAnchor, fragment_is_root, parse_base_url, and URI collisions
        assert!(registry.register_openapi_yaml("u", "\t").is_err());
        assert!(registry.register_schema_yaml("u", "\t").is_err());
        assert!(registry
            .register_openapi_json("u", serde_json::json!("not_an_obj"))
            .is_err());
        assert!(registry
            .register_schema_json("u", serde_json::json!("not_a_schema"))
            .is_err());
        assert!(registry
            .register_openapi_json(
                "u",
                serde_json::json!({
                    "openapi": "3.1.0",
                    "info": { "title": "T", "version": "1" },
                    "components": { "schemas": { "Bad": { "type": 123 } } }
                })
            )
            .is_err());

        // URI collision
        assert!(registry
            .register_openapi_yaml("https://example.com/openapi.yaml", openapi)
            .is_err());

        let full_openapi = r#"
openapi: 3.1.0
$self: https://example.com/full.yaml
info:
  title: Full
  version: "1.0"
paths:
  /items:
    get:
      responses:
        '200':
          description: ok
components:
  schemas:
    CompSchema:
      $id: https://example.com/schemas/CompSchema
      $anchor: CompAnchor
      type: string
    RefSchema:
      $id: https://example.com/schemas/RefSchemaId
      $anchor: RefSchemaAnchor
      $ref: '#/components/schemas/CompSchema'
  pathItems:
    InlinePi:
      get:
        responses:
          '200':
            description: ok
    RefPi:
      $ref: '#/components/pathItems/InlinePi'
  securitySchemes:
    ApiKey:
      type: apiKey
      name: X-Key
      in: header
  parameters:
    Param1:
      name: p
      in: query
      schema:
        type: string
"#;
        registry
            .register_openapi_yaml("/local/full.yaml", full_openapi)
            .expect("register full");

        let base_full = Url::parse("https://example.com/full.yaml").expect("url");
        // Resolve component schema by $id, $anchor (with doc and with base), and JSON pointer
        assert!(registry
            .resolve_schema_ref("https://example.com/schemas/CompSchema#/", None)
            .is_some());
        assert!(registry
            .resolve_schema_ref("https://example.com/schemas/CompSchema#/not_root", None)
            .is_none());
        assert!(registry
            .resolve_schema_ref("#CompAnchor", Some(&base_full))
            .is_some());
        assert!(registry
            .resolve_schema_ref("https://example.com/full.yaml#CompAnchor", None)
            .is_some());
        assert!(registry
            .resolve_schema_ref(
                "https://example.com/full.yaml#/components/schemas/CompSchema",
                None
            )
            .is_some());
        assert!(registry
            .resolve_schema_ref(
                "https://example.com/full.yaml#/components/schemas/RefSchema",
                None
            )
            .is_none());
        assert!(registry
            .resolve_schema_ref("https://example.com/full.yaml#/other/schemas/X", None)
            .is_none());
        assert!(registry
            .resolve_schema_ref("https://example.com/full.yaml#NotPointer", None)
            .is_none());
        assert!(registry
            .resolve_schema_ref("https://example.com/full.yaml", None)
            .is_none());
        assert!(registry.resolve_schema_ref("#/local/only", None).is_none());

        // Resolve inline schema anchor with doc_uri and base
        let base_inline = Url::parse("https://example.com/openapi.yaml").expect("url");
        assert!(registry
            .resolve_schema_ref("https://example.com/openapi.yaml#InlineAnchor", None)
            .is_some());
        assert!(registry
            .resolve_schema_ref("#InlineAnchor", Some(&base_inline))
            .is_some());
        assert!(registry
            .resolve_schema_ref("https://example.com/schemas/Inline#/sub", None)
            .is_none());

        // Resolve root schema by doc URI, with #/, and non-root fragment
        registry
            .register_schema_yaml(
                "https://example.com/schemas/base.json",
                "type: object\nproperties:\n  id:\n    type: string\n",
            )
            .expect("base schema");
        assert!(registry
            .resolve_schema_ref("https://example.com/schemas/base.json", None)
            .is_some());
        assert!(registry
            .resolve_schema_ref("https://example.com/schemas/base.json#", None)
            .is_some());
        assert!(registry
            .resolve_schema_ref("https://example.com/schemas/base.json#/", None)
            .is_some());
        assert!(registry
            .resolve_schema_ref("https://example.com/schemas/base.json#/sub", None)
            .is_none());

        // Register root schema with $dynamicAnchor, empty anchor, and relative retrieval URI
        registry
            .register_schema_yaml(
                "rel/dyn.json",
                r#"
$id: dyn_id.json
$anchor: "   "
$dynamicAnchor: DynRoot
type: boolean
"#,
            )
            .expect("dyn schema");
        assert!(registry.resolve_schema_ref("#DynRoot", None).is_some());
        assert!(registry
            .resolve_schema_ref("http://example.invalid/rel/dyn_id.json", None)
            .is_some());

        // Resolve component_ref & component_ref_with_components
        assert!(registry
            .resolve_component_ref(
                "#/components/pathItems/InlinePi",
                Some(&base_full),
                "pathItems"
            )
            .is_some());
        assert!(registry
            .resolve_component_ref(
                "https://example.com/full.yaml#/components/securitySchemes/ApiKey",
                None,
                "securitySchemes"
            )
            .is_some());
        assert!(registry
            .resolve_component_ref(
                "https://example.com/full.yaml#/components/parameters/Param1",
                None,
                "parameters"
            )
            .is_some());
        assert!(registry
            .resolve_component_ref("https://example.com/full.yaml#bad", None, "parameters")
            .is_none());
        assert!(registry
            .resolve_component_ref("https://example.com/full.yaml#/other/p", None, "parameters")
            .is_none());
        assert!(registry
            .resolve_component_ref(
                "https://example.com/schemas/base.json#/components/parameters/P",
                None,
                "parameters"
            )
            .is_none());
        assert!(registry
            .resolve_component_ref_with_components(
                "#/components/parameters/Param1",
                Some(&base_full),
                "parameters"
            )
            .is_some());
        assert!(registry
            .resolve_component_ref_with_components(
                "https://example.com/full.yaml#/components/parameters/Param1",
                None,
                "parameters"
            )
            .is_some());
        assert!(registry
            .resolve_component_ref_with_components(
                "https://example.com/missing_doc.yaml#/components/parameters/Param1",
                None,
                "parameters"
            )
            .is_none());
        assert!(registry
            .resolve_component_ref_with_components(
                "https://example.com/full.yaml#/components/parameters/MissingParam",
                None,
                "parameters"
            )
            .is_none());
        assert!(registry
            .resolve_component_ref_with_components(
                "#/components/parameters/Param1",
                None,
                "parameters"
            )
            .is_none());
        assert!(registry
            .resolve_path_item_ref("https://example.com/missing_doc.yaml#/paths/~1items", None)
            .is_none());
        assert!(registry
            .resolve_path_item_ref("#/paths/~1items", None)
            .is_none());

        // Resolve path_item_ref & path_item_ref_with_components
        assert!(registry
            .resolve_path_item_ref("#/components/pathItems/InlinePi", Some(&base_full))
            .is_some());
        assert!(registry
            .resolve_path_item_ref(
                "https://example.com/full.yaml#/components/pathItems/RefPi",
                None
            )
            .is_some());
        assert!(registry
            .resolve_path_item_ref("https://example.com/full.yaml#/paths/~1items", None)
            .is_some());
        assert!(registry
            .resolve_path_item_ref("https://example.com/full.yaml#/paths/~1missing", None)
            .is_none());
        assert!(registry
            .resolve_path_item_ref("https://example.com/full.yaml#/components/other/X", None)
            .is_none());
        assert!(registry
            .resolve_path_item_ref("https://example.com/full.yaml#bad", None)
            .is_none());
        assert!(registry
            .resolve_path_item_ref("https://example.com/schemas/base.json#/paths/~1items", None)
            .is_none());

        assert!(registry
            .resolve_schema_ref("https://example.com/schemas/RefSchemaId", None)
            .is_none());
        assert!(registry
            .resolve_schema_ref("#RefSchemaAnchor", Some(&base_full))
            .is_none());
        assert!(registry.resolve_schema_ref("http://[::1", None).is_none());

        // Register root schema with unparseable retrieval URI and relative $id
        registry
            .register_schema_yaml(
                "http://[::1",
                r#"
$id: rel_no_base.json
$anchor: NoBaseAnchor
type: string
"#,
            )
            .expect("no base schema");
        assert!(registry.resolve_schema_ref("#NoBaseAnchor", None).is_some());

        // Cover empty fragment on $id and inline $id, unresolvable relative doc on anchor/inline anchor,
        // missing pathItem / securityScheme in component_value, and missing pathItem in resolve_path_item_ref
        assert!(registry
            .resolve_schema_ref("https://example.com/schemas/CompSchema#", None)
            .is_some());
        assert!(registry
            .resolve_schema_ref("https://example.com/schemas/Inline#", None)
            .is_some());
        assert!(registry
            .resolve_schema_ref("https://example.com/schemas/Inline#/", None)
            .is_some());
        assert!(registry
            .resolve_schema_ref("rel_doc.yaml#MissingAnchor", None)
            .is_none());
        assert!(registry
            .resolve_schema_ref(
                "https://example.com/full.yaml#/components/parameters/Param1",
                None
            )
            .is_none());
        assert!(registry
            .resolve_path_item_ref(
                "https://example.com/full.yaml#/components/pathItems/MissingPi",
                None
            )
            .is_none());
        assert!(registry
            .resolve_component_ref(
                "https://example.com/full.yaml#/components/other/Param1",
                None,
                "parameters"
            )
            .is_none());
        assert!(registry
            .resolve_component_ref(
                "https://example.com/full.yaml#/components/pathItems/MissingPi",
                None,
                "pathItems"
            )
            .is_none());
        assert!(registry
            .resolve_component_ref(
                "https://example.com/full.yaml#/components/securitySchemes/MissingSec",
                None,
                "securitySchemes"
            )
            .is_none());

        assert_eq!(
            resolve_base_url(
                "https://example.com/r.yaml",
                Some("https://example.com/s.yaml")
            ),
            Url::parse("https://example.com/s.yaml").ok()
        );
        assert_eq!(
            resolve_doc_uri("rel.yaml", Some(&base_full)).as_deref(),
            Some("https://example.com/rel.yaml")
        );
        assert_eq!(resolve_doc_uri("rel.yaml", None), None);
    }
}
