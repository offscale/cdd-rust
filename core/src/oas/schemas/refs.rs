#![deny(missing_docs)]

//! # Reference Resolution
//!
//! helper functions to resolve OpenAPI `Ref` objects.

use utoipa::openapi::{schema::Schema, Components, RefOr};

/// Resolves a `RefOr` to a `Schema` against `components`.
/// Only resolves one level deep to avoid infinite recursion loops in logic outside `collect_fields`.
///
/// # Arguments
///
/// * `ref_or` - The reference or inline schema.
/// * `components` - The components collection to resolve against.
pub(crate) fn resolve_ref_local<'a>(
    ref_or: &'a RefOr<Schema>,
    components: &'a Components,
) -> Option<&'a Schema> {
    match ref_or {
        RefOr::T(s) => Some(s),
        RefOr::Ref(r) => {
            let name = extract_ref_name(&r.ref_location);
            resolve_ref_name(&name, components)
        }
    }
}

/// Resolves a schema name (simple string) to a Schema object in components.
///
/// # Arguments
///
/// * `name` - The simple name of the schema (e.g. "User").
/// * `components` - The components collection.
pub(crate) fn resolve_ref_name<'a>(name: &str, components: &'a Components) -> Option<&'a Schema> {
    components
        .schemas
        .get(name)
        .and_then(|ref_or| match ref_or {
            RefOr::T(s) => Some(s),
            RefOr::Ref(_) => None, // Double ref resolution avoided for simplicity
        })
}

/// Extracts the simple name from a reference string.
/// e.g. `#/components/schemas/User` -> `User`
pub(crate) fn extract_ref_name(ref_loc: &str) -> String {
    ref_loc
        .split('/')
        .next_back()
        .unwrap_or("Unknown")
        .to_string()
}
