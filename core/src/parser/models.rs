//! # Data Models
//!
//! definition of Intermediate Representation (IR) structures for parsed Rust code.

use std::collections::BTreeMap;

/// Serde-style rename rules used to derive JSON field and variant names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameRule {
    /// `camelCase`
    CamelCase,
    /// `snake_case`
    SnakeCase,
    /// `kebab-case`
    KebabCase,
    /// `PascalCase`
    PascalCase,
    /// `SCREAMING_SNAKE_CASE`
    ScreamingSnakeCase,
    /// `SCREAMING-KEBAB-CASE`
    ScreamingKebabCase,
    /// `lowercase`
    Lowercase,
    /// `UPPERCASE`
    Uppercase,
}

impl RenameRule {
    /// Parses a serde `rename_all` value into a known rule.
    pub fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "camelcase" => Some(Self::CamelCase),
            "snake_case" => Some(Self::SnakeCase),
            "kebab-case" => Some(Self::KebabCase),
            "pascalcase" => Some(Self::PascalCase),
            "screaming_snake_case" => Some(Self::ScreamingSnakeCase),
            "screaming-kebab-case" => Some(Self::ScreamingKebabCase),
            "lowercase" => Some(Self::Lowercase),
            "uppercase" => Some(Self::Uppercase),
            _ => None,
        }
    }

    /// Applies the rename rule to a Rust identifier.
    pub fn apply(&self, input: &str) -> String {
        match self {
            Self::Lowercase => input.to_ascii_lowercase(),
            Self::Uppercase => input.to_ascii_uppercase(),
            Self::CamelCase => {
                let words = split_words(input);
                if words.is_empty() {
                    return String::new();
                }
                let mut out = String::new();
                out.push_str(&words[0].to_ascii_lowercase());
                for word in words.iter().skip(1) {
                    out.push_str(&capitalize(word));
                }
                out
            }
            Self::PascalCase => split_words(input)
                .into_iter()
                .map(|w| capitalize(&w))
                .collect::<String>(),
            Self::SnakeCase => split_words(input)
                .into_iter()
                .map(|w| w.to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join("_"),
            Self::KebabCase => split_words(input)
                .into_iter()
                .map(|w| w.to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join("-"),
            Self::ScreamingSnakeCase => split_words(input)
                .into_iter()
                .map(|w| w.to_ascii_uppercase())
                .collect::<Vec<_>>()
                .join("_"),
            Self::ScreamingKebabCase => split_words(input)
                .into_iter()
                .map(|w| w.to_ascii_uppercase())
                .collect::<Vec<_>>()
                .join("-"),
        }
    }
}

fn split_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut prev_is_lower_or_digit = false;

    for ch in input.chars() {
        if ch == '_' || ch == '-' {
            if !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
            prev_is_lower_or_digit = false;
            continue;
        }

        let is_upper = ch.is_uppercase();
        if is_upper && prev_is_lower_or_digit && !current.is_empty() {
            words.push(current.clone());
            current.clear();
        }

        current.push(ch);
        prev_is_lower_or_digit = ch.is_lowercase() || ch.is_ascii_digit();
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::new();
    out.extend(first.to_uppercase());
    out.push_str(&chars.as_str().to_ascii_lowercase());
    out
}

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
    /// Optional serde rename_all rule applied to fields.
    pub rename_all: Option<RenameRule>,
    /// The list of fields.
    pub fields: Vec<ParsedField>,
    /// Whether the struct is marked as deprecated.
    pub is_deprecated: bool,
    /// Whether unknown fields should be rejected (serde `deny_unknown_fields`).
    pub deny_unknown_fields: bool,
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
    /// Optional serde rename_all rule applied to variants.
    pub rename_all: Option<RenameRule>,
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
    /// Default discriminator mapping (OAS 3.2+).
    /// Used when the discriminator property is missing or unmapped.
    pub discriminator_default_mapping: Option<String>,
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
            rename_all: None,
            fields: Vec::new(),
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
        };
        let e = ParsedEnum {
            name: "Pet".to_string(),
            description: None,
            rename: None,
            rename_all: None,
            tag: None,
            untagged: false,
            variants: Vec::new(),
            is_deprecated: false,
            external_docs: None,
            discriminator_mapping: None,
            discriminator_default_mapping: None,
        };

        let model_struct = ParsedModel::Struct(s);
        let model_enum = ParsedModel::Enum(e);

        assert_eq!(model_struct.name(), "User");
        assert_eq!(model_enum.name(), "Pet");
    }

    #[test]
    fn test_rename_rule_apply_variants() {
        assert_eq!(RenameRule::SnakeCase.apply("userId"), "user_id");
        assert_eq!(RenameRule::KebabCase.apply("userId"), "user-id");
        assert_eq!(RenameRule::ScreamingSnakeCase.apply("userId"), "USER_ID");
        assert_eq!(RenameRule::ScreamingKebabCase.apply("userId"), "USER-ID");
        assert_eq!(RenameRule::PascalCase.apply("user_id"), "UserId");
        assert_eq!(RenameRule::CamelCase.apply("user_id"), "userId");
        assert_eq!(RenameRule::Lowercase.apply("UserID"), "userid");
        assert_eq!(RenameRule::Uppercase.apply("userId"), "USERID");
    }
}
