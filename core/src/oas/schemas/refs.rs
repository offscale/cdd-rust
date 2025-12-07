#![deny(missing_docs)]

//! # Reference Resolution
//!
//! Utilities for parsing, normalizing, and resolving OpenAPI Reference Objects (`$ref`)
//! according to RFC 3986 and OAS 3.2 Appendix F "Base URI Determination".
//!
//! Handles:
//! - Local References (e.g., `#/components/schemas/User`)
//! - Relative File References (e.g., `../models.yaml#/User`)
//! - Remote URIs (e.g., `https://example.com/api.json#/User`)
//! - Swagger 2.0 Legacy References (e.g., `#/definitions/User`)
//! - `$self` keyword base URI determination.

use std::path::{Component, Path, PathBuf};
use utoipa::openapi::{Components, RefOr, Schema};

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
pub fn parse_reference(ref_str: &str) -> ParsedReference<'_> {
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

fn is_remote(uri: &str) -> bool {
    uri.starts_with("http://") || uri.starts_with("https://") || uri.starts_with("file://")
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
/// This resolves against the provided `components` if available.
/// Returns `None` if the reference cannot be resolved.
pub fn resolve_ref_local<'a>(
    ref_or: &'a RefOr<Schema>,
    components: &'a Components,
) -> Option<&'a Schema> {
    match ref_or {
        RefOr::T(s) => Some(s),
        RefOr::Ref(r) => resolve_ref_name(&r.ref_location, components),
    }
}

/// Resolves a reference string (e.g. `#/components/schemas/User`) to a specific `Schema`
/// inside the provided `Components` object.
///
/// Also accepts bare names (e.g. `User`) for compatibility with internal name-based lookups.
///
/// Returns `None` if the reference is remote, relative, or not found.
pub fn resolve_ref_name<'a>(ref_str: &str, components: &'a Components) -> Option<&'a Schema> {
    // 0. Fast path for bare component names (internal logic compatibility)
    // If string has no '/', '#', or '.', treat as direct map key lookup.
    if !ref_str.contains('/') && !ref_str.contains('#') && !ref_str.contains('.') {
        if let Some(found) = components.schemas.get(ref_str) {
            return match found {
                RefOr::T(s) => Some(s),
                RefOr::Ref(_) => None,
            };
        }
    }

    // 1. Full Parse
    let parsed = parse_reference(ref_str);

    if parsed.kind == ReferenceKind::Local {
        if let Some(frag) = parsed.fragment {
            // Frag: /components/schemas/User
            // We assume standard structure or Swagger 2.0 structure
            // But we only have access to `components.schemas` map (Name -> Schema).
            // So we just take the last part of the name.
            if let Some(name) = frag.split('/').next_back() {
                if let Some(found) = components.schemas.get(name) {
                    return match found {
                        RefOr::T(s) => Some(s),
                        // Explicitly avoid infinite recursion in simple resolver
                        RefOr::Ref(_) => None,
                    };
                }
            }
        }
    }
    None
}

/// Determines the effective Base URI for a document.
///
/// Implements OAS 3.2 Appendix F "Base URI Within Content".
///
/// # Arguments
///
/// * `retrieval_uri` - The URI used to retrieve the document (or absolute file path).
/// * `self_val` - The value of the `$self` field in the document (if present).
///
/// # Logic
///
/// 1. If `$self` is present and absolute, it becomes the Base URI.
/// 2. If `$self` is present and relative, it is resolved against `retrieval_uri`.
/// 3. If `$self` is absent, `retrieval_uri` is the Base URI.
pub fn compute_base_uri(retrieval_uri: &str, self_val: Option<&str>) -> String {
    match self_val {
        Some(s) => {
            // If self_val is an absolute URI (Remote), resolve_uri_reference returns it.
            // If self_val is relative, resolve_uri_reference merges it with retrieval_uri.
            resolve_uri_reference(retrieval_uri, s)
        }
        None => retrieval_uri.to_string(),
    }
}

/// Resolves a relative reference against a base URI to produce a target URI.
///
/// Implements basic path normalization (RFC 3986 dot-segment removal) for file paths.
/// Does NOT handle network operations.
///
/// # Arguments
///
/// * `base_uri` - The absolute URI or file path of the current document.
/// * `ref_uri` - The relative reference found in the document.
///
/// # Returns
///
/// A resolved, normalized path string.
pub fn resolve_uri_reference(base_uri: &str, ref_uri: &str) -> String {
    let parsed_ref = parse_reference(ref_uri);

    match parsed_ref.kind {
        // If the ref is absolute (Remote), return it as is (minus fragment if you want document only,
        // but here we usually want the full locator)
        ReferenceKind::Remote => parsed_ref.document.to_string(),
        // If the ref is local, it belongs to the base document
        ReferenceKind::Local => base_uri.to_string(),
        // If relative, merge with base
        ReferenceKind::Relative => {
            if is_remote(base_uri) {
                // Naive URL resolution for http bases (without full URI parser dep)
                // Assumes base ends with file or slash.
                // This is a "best effort" compliant implementation purely with std.
                let base_path = if base_uri.ends_with('/') {
                    base_uri
                } else {
                    // Strip the file part: http://site.com/api/v1/spec.yaml -> http://site.com/api/v1/
                    match base_uri.rfind('/') {
                        Some(idx) => &base_uri[..=idx],
                        None => base_uri,
                    }
                };

                // If ref_uri starts with '/' (absolute path relative to host), simpler concat fails logic.
                // e.g. base: http://a.com/b/c, ref: /d. Should be http://a.com/d.
                // Our naive impl: http://a.com/b//d (which is often valid but not canonical).
                // Without `url` crate, we stick to safe naive appending or slight cleanup.
                if parsed_ref.document.starts_with('/') {
                    // Try to find scheme end "://" to identify host root
                    if let Some(scheme_end) = base_path.find("://") {
                        let after_scheme = &base_path[scheme_end + 3..];
                        if let Some(first_slash) = after_scheme.find('/') {
                            // http://host...
                            let root = &base_path[..scheme_end + 3 + first_slash];
                            return format!("{}{}", root, parsed_ref.document);
                        }
                    }
                }

                format!("{}{}", base_path, parsed_ref.document)
            } else {
                // File system resolution
                let base_path = Path::new(base_uri);
                let parent = base_path.parent().unwrap_or_else(|| Path::new("."));
                let joined = parent.join(parsed_ref.document);
                normalize_path(&joined).to_string_lossy().to_string()
            }
        }
    }
}

/// Normalize a path removing component like `.` and `..`.
///
/// Borrowed concept from Cargo's path normalization logic.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = path.components().peekable();
    let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek().cloned() {
        components.next();
        PathBuf::from(c.as_os_str())
    } else {
        PathBuf::new()
    };

    for component in components {
        match component {
            Component::Prefix(..) => unreachable!(),
            Component::RootDir => {
                ret.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                ret.pop();
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    ret
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_local_ref() {
        let r = parse_reference("#/components/schemas/User");
        assert_eq!(r.kind, ReferenceKind::Local);
        assert_eq!(r.document, "");
        assert_eq!(r.fragment, Some("/components/schemas/User"));
    }

    #[test]
    fn test_parse_relative_ref() {
        let r = parse_reference("../common/models.yaml#/Error");
        assert_eq!(r.kind, ReferenceKind::Relative);
        assert_eq!(r.document, "../common/models.yaml");
        assert_eq!(r.fragment, Some("/Error"));
    }

    #[test]
    fn test_parse_remote_ref() {
        let r = parse_reference("https://example.com/api.yaml#/Info");
        assert_eq!(r.kind, ReferenceKind::Remote);
        assert_eq!(r.document, "https://example.com/api.yaml");
        assert_eq!(r.fragment, Some("/Info"));
    }

    #[test]
    fn test_extract_name_local() {
        assert_eq!(extract_ref_name("#/components/schemas/User"), "User");
        // Swagger 2.0
        assert_eq!(extract_ref_name("#/definitions/Pet"), "Pet");
    }

    #[test]
    fn test_extract_name_relative() {
        assert_eq!(extract_ref_name("blob.yaml#/Blob"), "Blob");
        assert_eq!(extract_ref_name("blob.yaml"), "blob");
    }

    #[test]
    fn test_resolve_uri_file_system() {
        let base = "/home/user/project/api/openapi.yaml";
        let relative = "../models/user.yaml";

        // Expected: /home/user/project/models/user.yaml
        let resolved = resolve_uri_reference(base, relative);

        assert!(resolved.ends_with("models/user.yaml"));
        assert!(!resolved.contains(".."));
    }

    #[test]
    fn test_resolve_ref_local_via_components() {
        // Setup Components
        let mut components = Components::new();
        components.schemas.insert(
            "User".to_string(),
            RefOr::T(Schema::Object(utoipa::openapi::Object::new())),
        );

        // Test Full Ref
        let resolved = resolve_ref_name("#/components/schemas/User", &components);
        assert!(resolved.is_some());

        // Test Bare Name (internal usage support)
        let resolved_bare = resolve_ref_name("User", &components);
        match resolved_bare {
            Some(_) => {}
            None => panic!("Should resolve bare name 'User' in 'resolve_ref_name'"),
        }

        // Test Missing
        let resolved_none = resolve_ref_name("#/components/schemas/Missing", &components);
        assert!(resolved_none.is_none());
    }

    #[test]
    fn test_compute_base_uri_appendix_f_absolute() {
        // Appendix F Example 1: Absolute $self inside content
        let retrieval = "file://home/someone/src/api/openapi.yaml";
        let self_val = Some("https://example.com/api/openapi");

        // $self is absolute, so it overrides retrieval completely
        let base = compute_base_uri(retrieval, self_val);
        assert_eq!(base, "https://example.com/api/openapi");
    }

    #[test]
    fn test_compute_base_uri_appendix_f_relative() {
        // Appendix F Example with Relative $self
        let retrieval = "https://staging.example.com/api/shared/foo";
        let self_val = Some("/api/schemas/foo");

        // $self is relative path. Resolved against retrieval root.
        let base = compute_base_uri(retrieval, self_val);
        // retrieval root: https://staging.example.com
        // joined: https://staging.example.com/api/schemas/foo
        assert_eq!(base, "https://staging.example.com/api/schemas/foo");
    }

    #[test]
    fn test_compute_base_uri_absent_self() {
        let retrieval = "https://example.com/api/openapis.yaml";
        let base = compute_base_uri(retrieval, None);
        assert_eq!(base, retrieval);
    }
}
