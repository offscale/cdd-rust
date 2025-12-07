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

use std::fmt;
use std::path::Path;
use url::Url;
use utoipa::openapi::{Components, RefOr, Schema};

/// Context passed down during parsing to handle Base URI resolution.
#[derive(Clone)]
pub struct ResolutionContext<'a> {
    /// The Base URI established by `$self` or the retrieval URI.
    pub base_uri: Option<Url>,
    /// Access to global components for fragment resolution.
    pub components: &'a Components,
}

impl<'a> ResolutionContext<'a> {
    /// Creates a new context.
    pub fn new(base_uri_str: Option<String>, components: &'a Components) -> Self {
        let base_uri = base_uri_str.and_then(|s| Url::parse(&s).ok());
        Self {
            base_uri,
            components,
        }
    }
}

/// Manual Debug implementation since `utoipa::openapi::Components` does not implement Debug.
impl<'a> fmt::Debug for ResolutionContext<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolutionContext")
            .field("base_uri", &self.base_uri)
            .field("components", &"Components { ... }")
            .finish()
    }
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
                    if ref_url.scheme() == base.scheme()
                        && ref_url.host() == base.host()
                        && ref_url.path() == base.path()
                        && ref_url.port() == base.port()
                    {
                        return resolve_local_fragment(parsed.fragment, components);
                    }
                }
            }
            None
        }
        ReferenceKind::Relative => {
            // Relative references are generally external in this context,
            // unless we strictly implement "self" matching relative path logic (uncommon for schemas in single file).
            None
        }
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
            if let Some(found) = components.schemas.get(name) {
                return match found {
                    RefOr::T(s) => Some(s),
                    RefOr::Ref(_) => None, // Avoid infinite recursion in simple resolver
                };
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
}
