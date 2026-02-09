//! # Data Models
//!
//! definition of Intermediate Representation (IR) structures for parsed Rust code.

use std::collections::BTreeMap;

/// Represents a link to external documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedExternalDocs {
    /// The URL to the documentation.
    pub url: String,
    /// A short description of the target documentation.
    pub description: Option<String>,
}

/// Represents a field extracted from a struct or enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedField {
    /// The name of the field.
    pub name: String,
    /// The raw Rust type string.
    pub ty: String,
    /// Extracted doc comments (if any).
    pub description: Option<String>,
    /// The name override for JSON/Schema (e.g. from `#[serde(rename="...")]`).
    pub rename: Option<String>,
    /// Whether the field is marked to be skipped in serialization/schema.
    pub is_skipped: bool,
    /// Whether the field is marked as deprecated.
    pub is_deprecated: bool,
    /// External documentation associated with this field.
    pub external_docs: Option<ParsedExternalDocs>,
}

/// Represents a fully parsed struct including field and doc metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedStruct {
    /// The struct name.
    pub name: String,
    /// The struct-level description from doc comments.
    pub description: Option<String>,
    /// The struct name override (e.g. `#[oai(rename="...")]`).
    pub rename: Option<String>,
    /// The list of fields.
    pub fields: Vec<ParsedField>,
    /// Whether the struct is marked as deprecated.
    pub is_deprecated: bool,
    /// External documentation associated with this struct.
    pub external_docs: Option<ParsedExternalDocs>,
}

/// Represents a variant in an enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedVariant {
    /// The name of the variant (e.g., "Cat").
    pub name: String,
    /// The embedded type if tuple variant (e.g., "CatStruct").
    /// OpenAPI usually maps oneOf items to single-argument tuple variants.
    pub ty: Option<String>,
    /// Doc comments.
    pub description: Option<String>,
    /// Serde rename (primary identifier from mapping).
    pub rename: Option<String>,
    /// Serde aliases (alternative identifiers from mapping).
    pub aliases: Option<Vec<String>>,
    /// Whether the variant is marked as deprecated.
    pub is_deprecated: bool,
}

/// Represents a fully parsed enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedEnum {
    /// Enum name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Rename override.
    pub rename: Option<String>,
    /// Serde tag (e.g. `#[serde(tag = "type")]`).
    pub tag: Option<String>,
    /// Serde untagged flag.
    pub untagged: bool,
    /// Variants.
    pub variants: Vec<ParsedVariant>,
    /// Whether the enum is marked as deprecated.
    pub is_deprecated: bool,
    /// External documentation associated with this enum.
    pub external_docs: Option<ParsedExternalDocs>,
    /// Raw discriminator mapping dictionary (Value -> Ref).
    /// Useful for documentation purposes.
    pub discriminator_mapping: Option<BTreeMap<String, String>>,
}

/// Enum wrapper for either a Struct or an Enum model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedModel {
    /// A struct definition.
    Struct(ParsedStruct),
    /// An enum definition.
    Enum(ParsedEnum),
}

impl ParsedModel {
    /// Returns the name of the model.
    pub fn name(&self) -> &str {
        match self {
            ParsedModel::Struct(s) => &s.name,
            ParsedModel::Enum(e) => &e.name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parsed_model_name() {
        let s = ParsedStruct {
            name: "User".to_string(),
            description: None,
            rename: None,
            fields: Vec::new(),
            is_deprecated: false,
            external_docs: None,
        };
        let e = ParsedEnum {
            name: "Pet".to_string(),
            description: None,
            rename: None,
            tag: None,
            untagged: false,
            variants: Vec::new(),
            is_deprecated: false,
            external_docs: None,
            discriminator_mapping: None,
        };

        let model_struct = ParsedModel::Struct(s);
        let model_enum = ParsedModel::Enum(e);

        assert_eq!(model_struct.name(), "User");
        assert_eq!(model_enum.name(), "Pet");
    }
}
