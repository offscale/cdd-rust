#![deny(missing_docs)]

//! # Reference Resolution
//!
//! Utilities for parsing, normalizing, and resolving OpenAPI Reference Objects (`$ref`)
//! according to **RFC 3986** and OAS 3.2 Appendix F "Base URI Determination".
//!
//! Uses the `url` crate for strict standard compliance regarding:
//! - Dot-segment removal (`.`, `..`).
//! - Scheme-relative resolution.
//! - Query/Fragment merging.
//!
//! Handles:
//! - Local References (e.g., `#/components/schemas/User`)
//! - Relative File References (e.g., `../models.yaml#/User`)
//! - Remote URIs (e.g., `https://example.com/api.json#/User`)
//! - File Path normalization via URL conversion.
//! - **NEW**: Self-reference resolution via `$self` (OAS 3.2 Appendix F).

use crate::oas::normalization::{
    normalize_boolean_schemas, normalize_const_schemas, normalize_nullable_schemas,
};
use crate::oas::registry::DocumentRegistry;
use percent_encoding::percent_decode_str;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;
use url::Url;
use utoipa::openapi::{Components, RefOr, Schema};

const DUMMY_BASE: &str = "http://example.invalid/";

/// Context passed down during parsing to handle Base URI resolution.
#[derive(Clone)]
pub struct ResolutionContext<'a> {
    /// The Base URI established by `$self` or the retrieval URI.
    pub base_uri: Option<Url>,
    /// Access to global components for fragment resolution.
    pub components: &'a Components,
    /// Map of schema `$id` URIs to component schema names.
    pub schema_ids: HashMap<String, String>,
    /// Map of schema anchors (`$anchor`/`$dynamicAnchor`) to component schema names.
    pub schema_anchors: HashMap<String, String>,
    /// Map of inline schema `$id` URIs to parsed schemas.
    pub inline_schema_ids: HashMap<String, Schema>,
    /// Map of inline schema anchors (`$anchor`/`$dynamicAnchor`) to parsed schemas.
    pub inline_schema_anchors: HashMap<String, Schema>,
    /// Optional registry for resolving external references.
    pub registry: Option<&'a DocumentRegistry>,
}

impl<'a> ResolutionContext<'a> {
    /// Creates a new context.
    pub fn new(base_uri_str: Option<String>, components: &'a Components) -> Self {
        Self::with_registry(base_uri_str, components, None)
    }

    /// Creates a new context with an optional document registry.
    pub fn with_registry(
        base_uri_str: Option<String>,
        components: &'a Components,
        registry: Option<&'a DocumentRegistry>,
    ) -> Self {
        let base_uri = base_uri_str.and_then(|s| {
            if let Ok(url) = Url::parse(&s) {
                return Some(url);
            }
            let dummy = Url::parse(DUMMY_BASE).ok()?;
            if s.starts_with('/') {
                return dummy.join(&s).ok();
            }
            dummy.join(&s).ok()
        });
        Self {
            base_uri,
            components,
            schema_ids: HashMap::new(),
            schema_anchors: HashMap::new(),
            inline_schema_ids: HashMap::new(),
            inline_schema_anchors: HashMap::new(),
            registry,
        }
    }
}

/// Manual Debug implementation since `utoipa::openapi::Components` does not implement Debug.
impl<'a> fmt::Debug for ResolutionContext<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolutionContext")
            .field("base_uri", &self.base_uri)
            .field("components", &"Components { ... }")
            .field("inline_schema_ids", &self.inline_schema_ids.len())
            .field("inline_schema_anchors", &self.inline_schema_anchors.len())
            .field(
                "registry",
                &self.registry.map(|_| "DocumentRegistry { ... }"),
            )
            .finish()
    }
}

/// Index of inline schema `$id` and `$anchor` values.
#[derive(Default)]
pub struct InlineSchemaIndex {
    /// Map of resolved `$id` URIs to parsed schema instances.
    pub ids: HashMap<String, Schema>,
    /// Map of resolved `$anchor`/`$dynamicAnchor` fragments to parsed schema instances.
    pub anchors: HashMap<String, Schema>,
}

/// The kind of the URI reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceKind {
    /// A reference within the same document (starts with `#`).
    Local,
    /// A relative path to another file (e.g., `models.yaml`, `../common.yaml`).
    Relative,
    /// An absolute URI (e.g., `https://...`, `file:///...`).
    Remote,
}

/// A parsed representation of a `$ref` string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedReference<'a> {
    /// The original reference string.
    pub exact: &'a str,
    /// The type of reference (Local, Relative, Remote).
    pub kind: ReferenceKind,
    /// The document URI part (everything before the `#`).
    /// Empty for Local references.
    pub document: &'a str,
    /// The fragment part (everything after the `#`).
    /// None if no `#` exists.
    pub fragment: Option<&'a str>,
}

/// Parses a `$ref` string into its components.
///
/// This does not perform IO or full RFC resolution, but categorizes the reference
/// structure for internal routing (extraction vs resolution).
pub fn parse_reference(ref_str: &str) -> ParsedReference<'_> {
    // RFC 3986 3.5: Fragment separator is '#'
    if let Some((doc, frag)) = ref_str.split_once('#') {
        if doc.is_empty() {
            ParsedReference {
                exact: ref_str,
                kind: ReferenceKind::Local,
                document: doc,
                fragment: Some(frag),
            }
        } else if is_remote(doc) {
            ParsedReference {
                exact: ref_str,
                kind: ReferenceKind::Remote,
                document: doc,
                fragment: Some(frag),
            }
        } else {
            ParsedReference {
                exact: ref_str,
                kind: ReferenceKind::Relative,
                document: doc,
                fragment: Some(frag),
            }
        }
    } else {
        // No hash, whole string is document
        if is_remote(ref_str) {
            ParsedReference {
                exact: ref_str,
                kind: ReferenceKind::Remote,
                document: ref_str,
                fragment: None,
            }
        } else {
            ParsedReference {
                exact: ref_str,
                kind: ReferenceKind::Relative,
                document: ref_str,
                fragment: None,
            }
        }
    }
}

/// Simple heuristic checks if a string starts with a URI scheme.
fn is_remote(uri: &str) -> bool {
    // Regex-free check for common schemes or general scheme syntax (alpha + alnum/+-.)
    if let Some(colon) = uri.find(':') {
        // RFC 3986 3.1: scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )
        let scheme = &uri[..colon];
        if scheme.is_empty() {
            return false;
        }
        let first = scheme.chars().next().unwrap();
        if !first.is_ascii_alphabetic() {
            // Not a scheme (e.g. /abs/path or ./rel)
            return false;
        }
        // It looks like a scheme.
        return true;
    }
    false
}

/// Extracts a usable Rust type name from a reference string.
///
/// # Logic
/// 1. If a fragment exists, use the last segment of the JSON pointer.
/// 2. If no fragment, use the file stem of the document path.
/// 3. Fallback to "Unknown".
pub fn extract_ref_name(ref_str: &str) -> String {
    let parsed = parse_reference(ref_str);

    if let Some(frag) = parsed.fragment {
        // Standard JSON Pointer: /components/schemas/User -> User
        if let Some(name) = frag.split('/').next_back() {
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }

    if !parsed.document.is_empty() {
        // File path: schemas/user.yaml -> user
        let path = Path::new(parsed.document);
        if let Some(stem) = path.file_stem() {
            return stem.to_string_lossy().to_string();
        }
    }

    "Unknown".to_string()
}

/// Resolves a generic `RefOr<Schema>` to a concrete `Schema` if it is a local reference.
///
/// This utilizes the `ResolutionContext` to check if an absolute URI reference
/// actually points to the current document (via matching `$self`).
///
/// Returns `None` if the reference cannot be resolved locally.
pub fn resolve_ref_local<'a>(
    ref_or: &'a RefOr<Schema>,
    context: &'a ResolutionContext<'a>,
) -> Option<&'a Schema> {
    match ref_or {
        RefOr::T(s) => Some(s),
        RefOr::Ref(r) => resolve_ref_name(&r.ref_location, context),
    }
}

/// Resolves a reference string (e.g. `#/components/schemas/User`) to a specific `Schema`
/// inside the provided `ResolutionContext`.
///
/// Also handling OAS 3.2 Appendix F: If `$self` is set to `"https://example.com/api"`,
/// then `"https://example.com/api#/components/schemas/User"` resolves locally.
pub fn resolve_ref_name<'a>(
    ref_str: &str,
    context: &'a ResolutionContext<'a>,
) -> Option<&'a Schema> {
    let components = context.components;

    // 0. Fast path: Internal simple name (e.g. "User")
    if !ref_str.contains('/') && !ref_str.contains('#') && !ref_str.contains('.') {
        if let Some(found) = components.schemas.get(ref_str) {
            return match found {
                RefOr::T(s) => Some(s),
                RefOr::Ref(_) => None,
            };
        }
    }

    let parsed = parse_reference(ref_str);

    // `$id`-aware resolution for schema references.
    if let Some(schema) = resolve_ref_via_schema_id(&parsed, context) {
        return Some(schema);
    }

    // `$anchor` / `$dynamicAnchor` resolution for schema references.
    if let Some(schema) = resolve_ref_via_schema_anchor(&parsed, context) {
        return Some(schema);
    }

    // External registry resolution.
    if let Some(registry) = context.registry {
        if let Some(schema) = registry.resolve_schema_ref(ref_str, context.base_uri.as_ref()) {
            return Some(schema);
        }
    }

    match parsed.kind {
        ReferenceKind::Local => {
            // Standard internal reference
            resolve_local_fragment(parsed.fragment, components)
        }
        ReferenceKind::Remote => {
            // Check if Remote text matches Base URI (Appendix F)
            if let Some(base) = &context.base_uri {
                // We parse the document part of the reference as a URL
                if let Ok(ref_url) = Url::parse(parsed.document) {
                    // Compare scheme, host, path, port
                    // If they match, we treat this as a local resolution utilizing the fragment.
                    if same_document(base, &ref_url) {
                        return resolve_local_fragment(parsed.fragment, components);
                    }
                }
            }
            None
        }
        ReferenceKind::Relative => {
            if let Some(base) = &context.base_uri {
                if let Ok(resolved_doc) = base.join(parsed.document) {
                    if same_document(base, &resolved_doc) {
                        return resolve_local_fragment(parsed.fragment, components);
                    }
                }
            }
            None
        }
    }
}

fn resolve_ref_via_schema_id<'a>(
    parsed: &ParsedReference<'_>,
    context: &'a ResolutionContext<'a>,
) -> Option<&'a Schema> {
    if parsed.document.is_empty() {
        return None;
    }

    let doc_uri = resolve_ref_document_uri(parsed.document, context.base_uri.as_ref())?;
    if let Some(schema_name) = context.schema_ids.get(&doc_uri) {
        if let Some(frag) = parsed.fragment {
            if !frag.is_empty() && frag != "/" {
                return None;
            }
        }

        if let Some(RefOr::T(schema)) = context.components.schemas.get(schema_name) {
            return Some(schema);
        }
    }

    if let Some(schema) = context.inline_schema_ids.get(&doc_uri) {
        if let Some(frag) = parsed.fragment {
            if !frag.is_empty() && frag != "/" {
                return None;
            }
        }
        return Some(schema);
    }

    None
}

fn resolve_ref_via_schema_anchor<'a>(
    parsed: &ParsedReference<'_>,
    context: &'a ResolutionContext<'a>,
) -> Option<&'a Schema> {
    let frag = parsed.fragment?;
    if frag.is_empty() || frag.starts_with('/') {
        return None;
    }

    let mut candidates = Vec::new();
    candidates.push(format!("#{}", frag));

    if !parsed.document.is_empty() {
        if let Some(doc_uri) = resolve_ref_document_uri(parsed.document, context.base_uri.as_ref())
        {
            candidates.push(format!("{}#{}", doc_uri, frag));
        }
    } else if let Some(base) = context.base_uri.as_ref() {
        candidates.push(format!("{}#{}", base, frag));
    }

    for candidate in candidates {
        if let Some(schema_name) = context.schema_anchors.get(&candidate) {
            if let Some(RefOr::T(schema)) = context.components.schemas.get(schema_name) {
                return Some(schema);
            }
        }
        if let Some(schema) = context.inline_schema_anchors.get(&candidate) {
            return Some(schema);
        }
    }

    None
}

pub(crate) fn resolve_ref_document_uri(doc: &str, base: Option<&Url>) -> Option<String> {
    if let Ok(url) = Url::parse(doc) {
        return Some(url.to_string());
    }

    let base = base?;
    base.join(doc).ok().map(|u| u.to_string())
}

/// Collects `$id` values from raw component schemas and resolves them to absolute URIs.
pub(crate) fn collect_schema_ids(
    raw_schemas: Option<&serde_json::Map<String, serde_json::Value>>,
    base_uri: Option<&Url>,
) -> HashMap<String, String> {
    let mut ids = HashMap::new();
    let Some(map) = raw_schemas else {
        return ids;
    };

    for (name, schema) in map {
        let Some(id) = schema.get("$id").and_then(|v| v.as_str()) else {
            continue;
        };

        let resolved = if let Ok(url) = Url::parse(id) {
            url.to_string()
        } else if let Some(base) = base_uri {
            base.join(id)
                .ok()
                .map(|u| u.to_string())
                .unwrap_or_else(|| id.to_string())
        } else {
            id.to_string()
        };

        ids.insert(resolved, name.clone());
    }

    ids
}

/// Collects `$anchor` / `$dynamicAnchor` values from raw component schemas.
///
/// Anchors are resolved against the schema `$id` when present, otherwise against
/// the document base URI when available. We also register a fragment-only form
/// (`#anchor`) to support local references.
pub(crate) fn collect_schema_anchors(
    raw_schemas: Option<&serde_json::Map<String, serde_json::Value>>,
    base_uri: Option<&Url>,
) -> HashMap<String, String> {
    let mut anchors = HashMap::new();
    let Some(map) = raw_schemas else {
        return anchors;
    };

    for (name, schema) in map {
        let Some(obj) = schema.as_object() else {
            continue;
        };

        let base = obj
            .get("$id")
            .and_then(|v| v.as_str())
            .and_then(|id| resolve_ref_document_uri(id, base_uri))
            .or_else(|| base_uri.map(|u| u.to_string()));

        if let Some(anchor) = obj.get("$anchor").and_then(|v| v.as_str()) {
            insert_anchor(&mut anchors, anchor, base.as_deref(), name);
        }
        if let Some(anchor) = obj.get("$dynamicAnchor").and_then(|v| v.as_str()) {
            insert_anchor(&mut anchors, anchor, base.as_deref(), name);
        }
    }

    anchors
}

/// Returns true if a schema JSON value contains `$dynamicRef` anywhere in its tree.
pub(crate) fn contains_dynamic_ref(value: &JsonValue) -> bool {
    match value {
        JsonValue::Object(map) => {
            if map.contains_key("$dynamicRef") {
                return true;
            }
            map.values().any(contains_dynamic_ref)
        }
        JsonValue::Array(items) => items.iter().any(contains_dynamic_ref),
        _ => false,
    }
}

/// Resolves `$dynamicRef` values for a component schema and any referenced components.
///
/// This walks the schema tree rooted at `entry_name`, applies JSON Schema dynamic
/// anchor scoping rules, and replaces `$dynamicRef` with `$ref` targeting the
/// resolved `$dynamicAnchor` (or the normal reference target when no matching
/// dynamic anchor is in scope).
///
/// Returns a map of component names to updated schema JSON values.
pub(crate) fn resolve_dynamic_refs_for_component(
    entry_name: &str,
    raw_schemas: &serde_json::Map<String, JsonValue>,
    schema_ids: &HashMap<String, String>,
    document_base: Option<&Url>,
    self_uri: Option<&str>,
) -> HashMap<String, JsonValue> {
    let mut resolved = HashMap::new();
    let mut visiting = HashSet::new();
    let mut scope = DynamicScope::default();

    if raw_schemas.contains_key(entry_name) {
        resolve_component_dynamic_refs(
            entry_name,
            document_base,
            raw_schemas,
            schema_ids,
            self_uri,
            &mut scope,
            &mut resolved,
            &mut visiting,
        );
    }

    resolved
}

/// Resolves `$dynamicRef` values inside a schema-root document.
///
/// This applies the same dynamic anchor scoping rules as component resolution,
/// but does not attempt to follow component `$ref` targets (since there are no
/// `components` in schema-root documents).
pub(crate) fn resolve_dynamic_refs_in_schema_root(
    raw: &JsonValue,
    document_base: Option<&Url>,
) -> JsonValue {
    let mut cloned = raw.clone();
    let mut scope = DynamicScope::default();
    let mut resolved = HashMap::new();
    let mut visiting = HashSet::new();
    let empty = serde_json::Map::new();
    let empty_ids = HashMap::new();
    resolve_dynamic_refs_in_resource(
        &mut cloned,
        document_base,
        document_base,
        &empty,
        &empty_ids,
        None,
        &mut scope,
        &mut resolved,
        &mut visiting,
    );
    cloned
}

#[derive(Clone, Default)]
struct DynamicScope {
    anchors: Vec<(String, String)>,
}

impl DynamicScope {
    fn push(&mut self, name: String, target: String) {
        self.anchors.push((name, target));
    }

    fn pop(&mut self, count: usize) {
        for _ in 0..count {
            self.anchors.pop();
        }
    }

    fn resolve(&self, name: &str) -> Option<&str> {
        self.anchors
            .iter()
            .find(|(anchor, _)| anchor == name)
            .map(|(_, target)| target.as_str())
    }
}

fn resolve_component_dynamic_refs(
    name: &str,
    document_base: Option<&Url>,
    raw_schemas: &serde_json::Map<String, JsonValue>,
    schema_ids: &HashMap<String, String>,
    self_uri: Option<&str>,
    scope: &mut DynamicScope,
    resolved: &mut HashMap<String, JsonValue>,
    visiting: &mut HashSet<String>,
) {
    if resolved.contains_key(name) || visiting.contains(name) {
        return;
    }

    let Some(schema) = raw_schemas.get(name) else {
        return;
    };

    visiting.insert(name.to_string());
    let mut cloned = schema.clone();
    resolve_dynamic_refs_in_resource(
        &mut cloned,
        document_base,
        document_base,
        raw_schemas,
        schema_ids,
        self_uri,
        scope,
        resolved,
        visiting,
    );
    resolved.insert(name.to_string(), cloned);
    visiting.remove(name);
}

fn resolve_dynamic_refs_in_resource(
    node: &mut JsonValue,
    current_base: Option<&Url>,
    document_base: Option<&Url>,
    raw_schemas: &serde_json::Map<String, JsonValue>,
    schema_ids: &HashMap<String, String>,
    self_uri: Option<&str>,
    scope: &mut DynamicScope,
    resolved: &mut HashMap<String, JsonValue>,
    visiting: &mut HashSet<String>,
) {
    let (resource_base_url, resource_base_str) = resource_base(node, current_base);
    let mut anchors = Vec::new();
    collect_resource_dynamic_anchors(node, resource_base_str.as_deref(), true, &mut anchors);
    let anchor_count = anchors.len();
    for (name, target) in anchors {
        scope.push(name, target);
    }

    resolve_dynamic_refs_walk(
        node,
        resource_base_url.as_ref(),
        document_base,
        raw_schemas,
        schema_ids,
        self_uri,
        scope,
        resolved,
        visiting,
    );

    scope.pop(anchor_count);
}

fn resolve_dynamic_refs_walk(
    node: &mut JsonValue,
    current_base: Option<&Url>,
    document_base: Option<&Url>,
    raw_schemas: &serde_json::Map<String, JsonValue>,
    schema_ids: &HashMap<String, String>,
    self_uri: Option<&str>,
    scope: &mut DynamicScope,
    resolved: &mut HashMap<String, JsonValue>,
    visiting: &mut HashSet<String>,
) {
    match node {
        JsonValue::Object(map) => {
            if let Some(dynamic_ref) = map.get("$dynamicRef").and_then(|v| v.as_str()) {
                let resolved_ref = resolve_dynamic_ref_value(dynamic_ref, current_base, scope);
                map.remove("$dynamicRef");
                map.insert("$ref".to_string(), JsonValue::String(resolved_ref));
            }

            if let Some(ref_str) = map.get("$ref").and_then(|v| v.as_str()) {
                if let Some(target_name) =
                    resolve_component_ref_name(ref_str, self_uri, schema_ids, current_base)
                {
                    resolve_component_dynamic_refs_for_ref(
                        &target_name,
                        ref_str,
                        current_base,
                        document_base,
                        raw_schemas,
                        schema_ids,
                        self_uri,
                        scope,
                        resolved,
                        visiting,
                    );
                }
            }

            for value in map.values_mut() {
                if value
                    .as_object()
                    .and_then(|obj| obj.get("$id"))
                    .and_then(|v| v.as_str())
                    .is_some()
                {
                    resolve_dynamic_refs_in_resource(
                        value,
                        current_base,
                        document_base,
                        raw_schemas,
                        schema_ids,
                        self_uri,
                        scope,
                        resolved,
                        visiting,
                    );
                    continue;
                }
                resolve_dynamic_refs_walk(
                    value,
                    current_base,
                    document_base,
                    raw_schemas,
                    schema_ids,
                    self_uri,
                    scope,
                    resolved,
                    visiting,
                );
            }
        }
        JsonValue::Array(items) => {
            for item in items {
                if item
                    .as_object()
                    .and_then(|obj| obj.get("$id"))
                    .and_then(|v| v.as_str())
                    .is_some()
                {
                    resolve_dynamic_refs_in_resource(
                        item,
                        current_base,
                        document_base,
                        raw_schemas,
                        schema_ids,
                        self_uri,
                        scope,
                        resolved,
                        visiting,
                    );
                    continue;
                }
                resolve_dynamic_refs_walk(
                    item,
                    current_base,
                    document_base,
                    raw_schemas,
                    schema_ids,
                    self_uri,
                    scope,
                    resolved,
                    visiting,
                );
            }
        }
        _ => {}
    }
}

fn resolve_component_dynamic_refs_for_ref(
    name: &str,
    ref_str: &str,
    current_base: Option<&Url>,
    document_base: Option<&Url>,
    raw_schemas: &serde_json::Map<String, JsonValue>,
    schema_ids: &HashMap<String, String>,
    self_uri: Option<&str>,
    scope: &mut DynamicScope,
    resolved: &mut HashMap<String, JsonValue>,
    visiting: &mut HashSet<String>,
) {
    if resolved.contains_key(name) || visiting.contains(name) {
        return;
    }

    let Some(schema) = raw_schemas.get(name) else {
        return;
    };

    let ref_base = resolve_ref_base(ref_str, current_base, document_base);
    visiting.insert(name.to_string());
    let mut cloned = schema.clone();
    resolve_dynamic_refs_in_resource(
        &mut cloned,
        ref_base.as_ref(),
        document_base,
        raw_schemas,
        schema_ids,
        self_uri,
        scope,
        resolved,
        visiting,
    );
    resolved.insert(name.to_string(), cloned);
    visiting.remove(name);
}

fn resolve_component_ref_name(
    ref_str: &str,
    self_uri: Option<&str>,
    schema_ids: &HashMap<String, String>,
    current_base: Option<&Url>,
) -> Option<String> {
    if let Some(name) = crate::oas::ref_utils::extract_component_name(ref_str, self_uri, "schemas")
    {
        return Some(name);
    }

    let parsed = parse_reference(ref_str);
    let doc_uri = resolve_ref_document_uri(parsed.document, current_base).or_else(|| {
        if parsed.document.is_empty() {
            None
        } else {
            Some(parsed.document.to_string())
        }
    })?;
    if let Some(name) = schema_ids.get(&doc_uri) {
        if let Some(frag) = parsed.fragment {
            if !frag.is_empty() && frag != "/" {
                return None;
            }
        }
        return Some(name.clone());
    }

    None
}

fn resolve_dynamic_ref_value(
    ref_str: &str,
    current_base: Option<&Url>,
    scope: &DynamicScope,
) -> String {
    let parsed = parse_reference(ref_str);
    if let Some(fragment) = parsed.fragment {
        if !fragment.is_empty() && !fragment.starts_with('/') {
            let anchor_name = decode_anchor_name(fragment);
            if let Some(target) = scope.resolve(&anchor_name) {
                return target.to_string();
            }
        }
    }

    resolve_ref_with_base(ref_str, current_base)
}

fn resolve_ref_with_base(ref_str: &str, base: Option<&Url>) -> String {
    let parsed = parse_reference(ref_str);
    if matches!(parsed.kind, ReferenceKind::Remote) {
        return ref_str.to_string();
    }

    let Some(base) = base else {
        return ref_str.to_string();
    };

    if parsed.document.is_empty() {
        if let Some(frag) = parsed.fragment {
            return format!("{}#{}", base, frag);
        }
        return base.to_string();
    }

    match base.join(parsed.document) {
        Ok(resolved) => {
            if let Some(frag) = parsed.fragment {
                format!("{}#{}", resolved, frag)
            } else {
                resolved.to_string()
            }
        }
        Err(_) => ref_str.to_string(),
    }
}

fn resolve_ref_base(
    ref_str: &str,
    current_base: Option<&Url>,
    document_base: Option<&Url>,
) -> Option<Url> {
    let parsed = parse_reference(ref_str);
    if !parsed.document.is_empty() {
        if let Some(doc_uri) = resolve_ref_document_uri(parsed.document, current_base) {
            return Url::parse(&doc_uri).ok();
        }
    }

    document_base.cloned().or_else(|| current_base.cloned())
}

fn resource_base(node: &JsonValue, current_base: Option<&Url>) -> (Option<Url>, Option<String>) {
    let Some(obj) = node.as_object() else {
        return (current_base.cloned(), current_base.map(|u| u.to_string()));
    };

    if let Some(id) = obj.get("$id").and_then(|v| v.as_str()) {
        let base_url = resolve_schema_base(id, current_base).or_else(|| current_base.cloned());
        let base_str = Some(resolve_schema_id_key(id, current_base));
        return (base_url, base_str);
    }

    (current_base.cloned(), current_base.map(|u| u.to_string()))
}

fn collect_resource_dynamic_anchors(
    node: &JsonValue,
    base_str: Option<&str>,
    is_root: bool,
    anchors: &mut Vec<(String, String)>,
) {
    match node {
        JsonValue::Object(map) => {
            let has_id = map.get("$id").and_then(|v| v.as_str()).is_some();
            if !is_root && has_id {
                return;
            }

            if let Some(anchor) = map.get("$dynamicAnchor").and_then(|v| v.as_str()) {
                let trimmed = anchor.trim();
                if !trimmed.is_empty() {
                    let target = match base_str {
                        Some(base) => format!("{}#{}", base, trimmed),
                        None => format!("#{}", trimmed),
                    };
                    anchors.push((trimmed.to_string(), target));
                }
            }

            for value in map.values() {
                collect_resource_dynamic_anchors(value, base_str, false, anchors);
            }
        }
        JsonValue::Array(items) => {
            for item in items {
                collect_resource_dynamic_anchors(item, base_str, false, anchors);
            }
        }
        _ => {}
    }
}

fn decode_anchor_name(fragment: &str) -> String {
    percent_decode_str(fragment)
        .decode_utf8_lossy()
        .into_owned()
}

fn insert_anchor(
    anchors: &mut HashMap<String, String>,
    anchor: &str,
    base: Option<&str>,
    name: &str,
) {
    let trimmed = anchor.trim();
    if trimmed.is_empty() {
        return;
    }

    anchors.insert(format!("#{}", trimmed), name.to_string());
    if let Some(base_uri) = base {
        anchors.insert(format!("{}#{}", base_uri, trimmed), name.to_string());
    }
}

/// Collects `$id` / `$anchor` / `$dynamicAnchor` targets from inline schemas.
///
/// This traverses the full OpenAPI or schema-root document, indexing any schema object
/// that declares an identifier or anchor. By default, component schema roots should
/// be skipped and indexed via `collect_schema_ids` / `collect_schema_anchors`.
pub(crate) fn collect_inline_schema_index(
    root: &JsonValue,
    base_uri: Option<&Url>,
    skip_component_schemas: bool,
) -> InlineSchemaIndex {
    let mut index = InlineSchemaIndex::default();
    let mut path = Vec::new();
    collect_inline_schema_index_inner(
        root,
        base_uri,
        skip_component_schemas,
        &mut path,
        &mut index,
    );
    index
}

fn collect_inline_schema_index_inner(
    node: &JsonValue,
    base_uri: Option<&Url>,
    skip_component_schemas: bool,
    path: &mut Vec<String>,
    index: &mut InlineSchemaIndex,
) {
    match node {
        JsonValue::Object(map) => {
            let is_component_schema_root = skip_component_schemas
                && path.len() == 3
                && path[0] == "components"
                && path[1] == "schemas";

            let id_value = map.get("$id").and_then(|v| v.as_str());
            let anchor_value = map.get("$anchor").and_then(|v| v.as_str());
            let dynamic_anchor_value = map.get("$dynamicAnchor").and_then(|v| v.as_str());

            let mut child_base = base_uri.cloned();
            if let Some(id) = id_value {
                if let Some(resolved) = resolve_schema_base(id, base_uri) {
                    child_base = Some(resolved);
                }
            }

            let has_schema_marker =
                id_value.is_some() || anchor_value.is_some() || dynamic_anchor_value.is_some();
            let schema = if has_schema_marker {
                parse_inline_schema(node)
            } else {
                None
            };

            if !is_component_schema_root {
                if let (Some(schema), Some(id)) = (schema.as_ref(), id_value) {
                    let key = resolve_schema_id_key(id, base_uri);
                    index.ids.insert(key, schema.clone());
                }

                if let Some(schema) = schema.as_ref() {
                    let base_str = child_base.as_ref().map(|u| u.to_string());
                    if let Some(anchor) = anchor_value {
                        insert_anchor_schema(
                            &mut index.anchors,
                            anchor,
                            base_str.as_deref(),
                            schema,
                        );
                    }
                    if let Some(anchor) = dynamic_anchor_value {
                        insert_anchor_schema(
                            &mut index.anchors,
                            anchor,
                            base_str.as_deref(),
                            schema,
                        );
                    }
                }
            }

            for (key, value) in map {
                path.push(key.clone());
                collect_inline_schema_index_inner(
                    value,
                    child_base.as_ref(),
                    skip_component_schemas,
                    path,
                    index,
                );
                path.pop();
            }
        }
        JsonValue::Array(items) => {
            for (idx, value) in items.iter().enumerate() {
                path.push(idx.to_string());
                collect_inline_schema_index_inner(
                    value,
                    base_uri,
                    skip_component_schemas,
                    path,
                    index,
                );
                path.pop();
            }
        }
        _ => {}
    }
}

fn parse_inline_schema(value: &JsonValue) -> Option<Schema> {
    let mut normalized = value.clone();
    normalize_nullable_schemas(&mut normalized);
    normalize_boolean_schemas(&mut normalized);
    normalize_const_schemas(&mut normalized);
    serde_json::from_value::<Schema>(normalized).ok()
}

fn resolve_schema_id_key(id: &str, base_uri: Option<&Url>) -> String {
    if let Ok(url) = Url::parse(id) {
        return url.to_string();
    }
    if let Some(base) = base_uri {
        return base
            .join(id)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| id.to_string());
    }
    id.to_string()
}

fn resolve_schema_base(id: &str, base_uri: Option<&Url>) -> Option<Url> {
    if let Ok(url) = Url::parse(id) {
        return Some(url);
    }
    let base = base_uri?;
    base.join(id).ok()
}

fn insert_anchor_schema(
    anchors: &mut HashMap<String, Schema>,
    anchor: &str,
    base: Option<&str>,
    schema: &Schema,
) {
    let trimmed = anchor.trim();
    if trimmed.is_empty() {
        return;
    }

    anchors.insert(format!("#{}", trimmed), schema.clone());
    if let Some(base_uri) = base {
        anchors.insert(format!("{}#{}", base_uri, trimmed), schema.clone());
    }
}

/// Internal helper to look up a Schema from a fragment string.
fn resolve_local_fragment<'a>(
    fragment: Option<&str>,
    components: &'a Components,
) -> Option<&'a Schema> {
    if let Some(frag) = fragment {
        // Frag: /components/schemas/User
        // We only support digging into schemas for code generation purposes.
        if let Some(name) = frag.split('/').next_back() {
            let decoded = crate::oas::ref_utils::decode_pointer_segment(name);
            if let Some(found) = components.schemas.get(&decoded) {
                return match found {
                    RefOr::T(s) => Some(s),
                    RefOr::Ref(_) => None, // Avoid infinite recursion in simple resolver
                };
            }
        }
    }
    None
}

fn same_document(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host() == right.host()
        && left.port() == right.port()
        && left.path() == right.path()
}

/// Determines the effective Base URI for a document.
///
/// Implements OAS 3.2 Appendix F "Base URI Within Content".
///
/// # Arguments
///
/// * `retrieval_uri` - The URI used to retrieve the document (or absolute file path).
/// * `self_val` - The value of the `$self` field in the document (if present).
pub fn compute_base_uri(retrieval_uri: &str, self_val: Option<&str>) -> String {
    match self_val {
        Some(s) => resolve_uri_reference(retrieval_uri, s),
        None => retrieval_uri.to_string(),
    }
}

/// Resolves a relative reference against a base URI utilizing **RFC 3986** logic.
///
/// Replaces previous naive string slicing with strict `Url::join`.
pub fn resolve_uri_reference(base_uri: &str, ref_uri: &str) -> String {
    // 1. Attempt to parse base as a strict URL (e.g. http://..., file://...)
    let base_url = match Url::parse(base_uri) {
        Ok(u) => u,
        Err(_) => {
            // 2. If parsing failed, assume it's a raw file path (OS specific).
            let path = Path::new(base_uri);
            let abs_path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir().unwrap_or_default().join(path)
            };

            match Url::from_file_path(&abs_path) {
                Ok(u) => u,
                Err(_) => {
                    // Fallback
                    if base_uri.ends_with('/') || base_uri.ends_with('\\') {
                        return format!("{}{}", base_uri, ref_uri);
                    } else {
                        return format!("{}/{}", base_uri, ref_uri);
                    }
                }
            }
        }
    };

    // 3. Perform RFC 3986 Join
    match base_url.join(ref_uri) {
        Ok(resolved) => {
            // 4. Convert back to file path if applicable
            if resolved.scheme() == "file" {
                if let Ok(p) = resolved.to_file_path() {
                    return p.to_string_lossy().to_string();
                }
            }
            resolved.to_string()
        }
        Err(_) => ref_uri.to_string(),
    }
}

/// Computes the directory of the given URI or Path.
pub fn resolve_uri_directory(uri: &str) -> String {
    resolve_uri_reference(uri, ".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use utoipa::openapi::schema::{ObjectBuilder, Type};
    use utoipa::openapi::Components;
    use utoipa::openapi::Schema;

    #[test]
    fn test_resolve_ref_local_strict() {
        let mut components = Components::new();
        components.schemas.insert(
            "User".to_string(),
            RefOr::T(Schema::Object(utoipa::openapi::schema::Object::default())),
        );
        let ctx = ResolutionContext::new(None, &components);

        let resolved = resolve_ref_name("#/components/schemas/User", &ctx);
        assert!(resolved.is_some());
    }

    #[test]
    fn test_resolve_ref_via_self_base_uri() {
        let mut components = Components::new();
        components.schemas.insert(
            "User".to_string(),
            RefOr::T(Schema::Object(utoipa::openapi::schema::Object::default())),
        );

        // Define $self as specific URI
        let ctx = ResolutionContext::new(
            Some("https://api.example.com/v1/openapi.yaml".to_string()),
            &components,
        );

        // Reference uses absolute URI that matches $self
        let ref_uri = "https://api.example.com/v1/openapi.yaml#/components/schemas/User";
        let resolved = resolve_ref_name(ref_uri, &ctx);

        assert!(
            resolved.is_some(),
            "Should resolve absolute URI matching Base URI"
        );
    }

    #[test]
    fn test_resolve_ref_relative_to_base_uri() {
        let mut components = Components::new();
        components.schemas.insert(
            "User".to_string(),
            RefOr::T(Schema::Object(utoipa::openapi::schema::Object::default())),
        );

        let ctx = ResolutionContext::new(
            Some("https://api.example.com/v1/openapi.yaml".to_string()),
            &components,
        );

        let ref_uri = "openapi.yaml#/components/schemas/User";
        let resolved = resolve_ref_name(ref_uri, &ctx);
        assert!(
            resolved.is_some(),
            "Should resolve relative ref against Base URI"
        );
    }

    #[test]
    fn test_resolve_ref_relative_to_absolute_path_self() {
        let mut components = Components::new();
        components.schemas.insert(
            "User".to_string(),
            RefOr::T(Schema::Object(utoipa::openapi::schema::Object::default())),
        );

        let ctx = ResolutionContext::new(Some("/api/openapi.yaml".to_string()), &components);

        let ref_uri = "openapi.yaml#/components/schemas/User";
        let resolved = resolve_ref_name(ref_uri, &ctx);
        assert!(
            resolved.is_some(),
            "Should resolve relative ref against absolute-path $self"
        );
    }

    #[test]
    fn test_resolve_ref_relative_to_relative_self() {
        let mut components = Components::new();
        components.schemas.insert(
            "User".to_string(),
            RefOr::T(Schema::Object(utoipa::openapi::schema::Object::default())),
        );

        let ctx = ResolutionContext::new(Some("./specs/openapi.yaml".to_string()), &components);

        let ref_uri = "openapi.yaml#/components/schemas/User";
        let resolved = resolve_ref_name(ref_uri, &ctx);
        assert!(
            resolved.is_some(),
            "Should resolve relative ref against relative $self"
        );
    }

    #[test]
    fn test_resolve_ref_mismatch_base_uri() {
        let components = Components::new();
        let ctx = ResolutionContext::new(
            Some("https://api.example.com/v1/openapi.yaml".to_string()),
            &components,
        );

        // Reference uses DIFFERENT absolute URI
        let ref_uri = "https://other.com/spec.yaml#/components/schemas/User";
        let resolved = resolve_ref_name(ref_uri, &ctx);

        assert!(
            resolved.is_none(),
            "Should NOT resolve mismatching absolute URI locally"
        );
    }

    #[test]
    fn test_parse_reference() {
        let r = parse_reference("https://example.com/api.yaml#/Info");
        assert_eq!(r.kind, ReferenceKind::Remote);
        assert_eq!(r.document, "https://example.com/api.yaml");
        assert_eq!(r.fragment, Some("/Info"));
    }

    #[test]
    fn test_collect_schema_ids_resolves_relative_ids() {
        let raw = json!({
            "User": { "$id": "/schemas/user" },
            "Token": { "$id": "schemas/token" }
        });
        let base = Url::parse("https://example.com/openapi.yaml").unwrap();
        let ids = collect_schema_ids(raw.as_object(), Some(&base));
        assert_eq!(
            ids.get("https://example.com/schemas/user"),
            Some(&"User".to_string())
        );
        assert_eq!(
            ids.get("https://example.com/schemas/token"),
            Some(&"Token".to_string())
        );
    }

    #[test]
    fn test_resolve_ref_name_via_schema_id() {
        let mut components = Components::new();
        let schema = Schema::Object(ObjectBuilder::new().schema_type(Type::String).build());
        components
            .schemas
            .insert("Token".to_string(), RefOr::T(schema));

        let mut ctx = ResolutionContext::new(
            Some("https://example.com/openapi.yaml".to_string()),
            &components,
        );
        ctx.schema_ids.insert(
            "https://example.com/schemas/token".to_string(),
            "Token".to_string(),
        );

        let resolved = resolve_ref_name("https://example.com/schemas/token", &ctx);
        assert!(matches!(resolved, Some(Schema::Object(_))));
    }

    #[test]
    fn test_dynamic_ref_resolves_to_referrer_anchor() {
        let raw = json!({
            "genericArrayComponent": {
                "$id": "fully_generic_array",
                "type": "array",
                "items": { "$dynamicRef": "#generic-array" },
                "$defs": {
                    "allowAll": { "$dynamicAnchor": "generic-array" }
                }
            },
            "numberArray": {
                "$id": "array_of_numbers",
                "$ref": "fully_generic_array",
                "$defs": {
                    "numbersOnly": {
                        "$dynamicAnchor": "generic-array",
                        "type": "number"
                    }
                }
            }
        });

        let raw_map = raw.as_object().unwrap();
        let schema_ids = collect_schema_ids(Some(raw_map), None);
        let resolved =
            resolve_dynamic_refs_for_component("numberArray", raw_map, &schema_ids, None, None);

        let generic = resolved
            .get("genericArrayComponent")
            .expect("resolved generic schema");
        let items_ref = generic
            .get("items")
            .and_then(|v| v.get("$ref"))
            .and_then(|v| v.as_str())
            .unwrap();

        assert_eq!(items_ref, "array_of_numbers#generic-array");
        assert!(generic
            .get("items")
            .and_then(|v| v.get("$dynamicRef"))
            .is_none());
    }

    #[test]
    fn test_dynamic_ref_resolves_to_local_anchor_when_unoverridden() {
        let raw = json!({
            "genericArrayComponent": {
                "$id": "fully_generic_array",
                "type": "array",
                "items": { "$dynamicRef": "#generic-array" },
                "$defs": {
                    "allowAll": { "$dynamicAnchor": "generic-array" }
                }
            }
        });

        let raw_map = raw.as_object().unwrap();
        let schema_ids = collect_schema_ids(Some(raw_map), None);
        let resolved = resolve_dynamic_refs_for_component(
            "genericArrayComponent",
            raw_map,
            &schema_ids,
            None,
            None,
        );

        let generic = resolved
            .get("genericArrayComponent")
            .expect("resolved generic schema");
        let items_ref = generic
            .get("items")
            .and_then(|v| v.get("$ref"))
            .and_then(|v| v.as_str())
            .unwrap();

        assert_eq!(items_ref, "fully_generic_array#generic-array");
    }

    #[test]
    fn test_collect_schema_anchors_registers_fragment() {
        let raw = json!({
            "User": { "$anchor": "UserAnchor", "type": "string" }
        });
        let base = Url::parse("https://example.com/openapi.yaml").unwrap();
        let anchors = collect_schema_anchors(raw.as_object(), Some(&base));
        assert_eq!(anchors.get("#UserAnchor"), Some(&"User".to_string()));
        assert_eq!(
            anchors.get("https://example.com/openapi.yaml#UserAnchor"),
            Some(&"User".to_string())
        );
    }

    #[test]
    fn test_resolve_ref_name_via_anchor() {
        let mut components = Components::new();
        let schema = Schema::Object(ObjectBuilder::new().schema_type(Type::String).build());
        components
            .schemas
            .insert("User".to_string(), RefOr::T(schema));

        let raw = json!({
            "User": { "$anchor": "UserAnchor", "type": "string" }
        });
        let base = Url::parse("https://example.com/openapi.yaml").unwrap();
        let anchors = collect_schema_anchors(raw.as_object(), Some(&base));

        let mut ctx = ResolutionContext::new(Some(base.to_string()), &components);
        ctx.schema_anchors = anchors;

        let resolved = resolve_ref_name("#UserAnchor", &ctx);
        assert!(matches!(resolved, Some(Schema::Object(_))));
    }

    #[test]
    fn test_resolve_ref_name_via_inline_schema_id_and_anchor() {
        let raw = json!({
            "openapi": "3.2.0",
            "info": { "title": "T", "version": "1.0" },
            "paths": {
                "/foo": {
                    "get": {
                        "responses": {
                            "200": {
                                "description": "ok",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "$id": "https://example.com/schemas/Inline",
                                            "$anchor": "InlineAnchor",
                                            "type": "string"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        let base = Url::parse("https://example.com/openapi.yaml").unwrap();
        let inline_index = collect_inline_schema_index(&raw, Some(&base), true);

        let components = Components::new();
        let mut ctx = ResolutionContext::new(Some(base.to_string()), &components);
        ctx.inline_schema_ids = inline_index.ids;
        ctx.inline_schema_anchors = inline_index.anchors;

        let resolved_by_id = resolve_ref_name("https://example.com/schemas/Inline", &ctx);
        assert!(matches!(resolved_by_id, Some(Schema::Object(_))));

        let resolved_by_anchor = resolve_ref_name("#InlineAnchor", &ctx);
        assert!(matches!(resolved_by_anchor, Some(Schema::Object(_))));

        let resolved_by_absolute_anchor =
            resolve_ref_name("https://example.com/schemas/Inline#InlineAnchor", &ctx);
        assert!(matches!(
            resolved_by_absolute_anchor,
            Some(Schema::Object(_))
        ));
    }

    #[test]
    fn test_collect_inline_schema_index_respects_component_skip() {
        let raw = json!({
            "openapi": "3.2.0",
            "info": { "title": "T", "version": "1.0" },
            "components": {
                "schemas": {
                    "User": {
                        "$id": "https://example.com/schemas/User",
                        "type": "string"
                    }
                }
            },
            "paths": {
                "/foo": {
                    "get": {
                        "responses": {
                            "200": {
                                "description": "ok",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "$id": "https://example.com/schemas/Inline",
                                            "type": "string"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        let base = Url::parse("https://example.com/openapi.yaml").unwrap();
        let skipped = collect_inline_schema_index(&raw, Some(&base), true);
        assert!(!skipped.ids.contains_key("https://example.com/schemas/User"));
        assert!(skipped
            .ids
            .contains_key("https://example.com/schemas/Inline"));

        let included = collect_inline_schema_index(&raw, Some(&base), false);
        assert!(included
            .ids
            .contains_key("https://example.com/schemas/User"));
    }
}
