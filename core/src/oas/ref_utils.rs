#![deny(missing_docs)]

//! # Reference Utilities
//!
//! Shared helpers for resolving `$ref` targets with respect to OAS 3.2 `$self`.
//!
//! These utilities are intentionally lightweight: they never fetch external documents,
//! but allow absolute or relative references to be treated as local when the
//! document part matches the current document's `$self` URI.

use crate::oas::schemas::refs::{parse_reference, ReferenceKind};
use percent_encoding::percent_decode_str;
use std::path::Path;
use url::Url;

/// Normalizes a `$ref` to a local JSON Pointer (e.g. `#/components/...`) if it targets the
/// current document as identified by `$self`.
///
/// Returns `None` if the reference is external or lacks a fragment.
pub(crate) fn normalize_ref_to_local(ref_str: &str, self_uri: Option<&str>) -> Option<String> {
    if ref_str.starts_with("#/") || ref_str == "#" {
        return Some(ref_str.to_string());
    }

    let parsed = parse_reference(ref_str);
    match parsed.kind {
        ReferenceKind::Local => Some(ref_str.to_string()),
        ReferenceKind::Relative | ReferenceKind::Remote => {
            let frag = parsed.fragment?;
            let self_uri = self_uri?;
            if ref_doc_matches_self(parsed.document, self_uri) {
                return Some(format!("#{}", frag));
            }
            None
        }
    }
}

/// Extracts a component name from a `$ref` if it points to `#/components/{section}/{name}`.
///
/// Returns `None` if the reference is not local to the current document.
pub(crate) fn extract_component_name(
    ref_str: &str,
    self_uri: Option<&str>,
    section: &str,
) -> Option<String> {
    let local = normalize_ref_to_local(ref_str, self_uri)?;
    let pointer = local.trim_start_matches('#').trim_start_matches('/');
    let segments: Vec<&str> = pointer.split('/').collect();

    if segments.len() < 3 {
        return None;
    }
    if segments[0] != "components" || segments[1] != section {
        return None;
    }

    let name = decode_pointer_segment(segments[2]);
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Decodes a JSON Pointer segment (handles `~1` and `~0`).
pub(crate) fn decode_pointer_segment(segment: &str) -> String {
    let decoded = segment.replace("~1", "/").replace("~0", "~");
    percent_decode_str(&decoded)
        .decode_utf8_lossy()
        .into_owned()
}

fn ref_doc_matches_self(ref_doc: &str, self_uri: &str) -> bool {
    if ref_doc == self_uri {
        return true;
    }

    match (Url::parse(ref_doc), Url::parse(self_uri)) {
        (Ok(ref_url), Ok(self_url)) => {
            return ref_url.scheme() == self_url.scheme()
                && ref_url.host() == self_url.host()
                && ref_url.port() == self_url.port()
                && ref_url.path() == self_url.path();
        }
        _ => {}
    }

    // If `$self` is an absolute-path reference (e.g. "/api/openapi"), compare path.
    if self_uri.starts_with('/') {
        if let Ok(ref_url) = Url::parse(ref_doc) {
            return ref_url.path() == self_uri;
        }
    }

    // Fallback: compare raw relative paths.
    if !self_uri.contains("://") && !ref_doc.contains("://") {
        return Path::new(ref_doc) == Path::new(self_uri);
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_ref_local_passthrough() {
        let normalized = normalize_ref_to_local("#/components/schemas/User", None).unwrap();
        assert_eq!(normalized, "#/components/schemas/User");
    }

    #[test]
    fn test_normalize_ref_self_absolute_match() {
        let self_uri = Some("https://example.com/openapi.yaml");
        let ref_str = "https://example.com/openapi.yaml#/components/schemas/User";
        let normalized = normalize_ref_to_local(ref_str, self_uri).unwrap();
        assert_eq!(normalized, "#/components/schemas/User");
    }

    #[test]
    fn test_normalize_ref_self_path_match() {
        let self_uri = Some("/api/openapi.yaml");
        let ref_str = "https://example.com/api/openapi.yaml#/components/schemas/User";
        let normalized = normalize_ref_to_local(ref_str, self_uri).unwrap();
        assert_eq!(normalized, "#/components/schemas/User");
    }

    #[test]
    fn test_extract_component_name_success() {
        let self_uri = Some("https://example.com/openapi.yaml");
        let ref_str = "https://example.com/openapi.yaml#/components/parameters/Limit";
        let name = extract_component_name(ref_str, self_uri, "parameters").unwrap();
        assert_eq!(name, "Limit");
    }

    #[test]
    fn test_decode_pointer_segment_percent_encoding() {
        let encoded = "User%20Profile~1details";
        let decoded = decode_pointer_segment(encoded);
        assert_eq!(decoded, "User Profile/details");
    }

    #[test]
    fn test_extract_component_name_wrong_section() {
        let self_uri = Some("https://example.com/openapi.yaml");
        let ref_str = "https://example.com/openapi.yaml#/components/responses/Limit";
        let name = extract_component_name(ref_str, self_uri, "parameters");
        assert!(name.is_none());
    }
}
