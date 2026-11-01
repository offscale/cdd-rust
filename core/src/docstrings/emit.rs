#![deny(missing_docs)]

//! # Docstring Emission
//!
//! Formats description strings into idiomatic Rust outer (`///`) and inner (`//!`) doc comments.

/// Formats a multi-line description string as outer Rust doc comments (`///`).
///
/// # Arguments
/// * `description` - The text to format.
/// * `indent` - The leading whitespace indentation prefix for each line.
///
/// # Returns
/// * Formatted doc comment block ending with a newline, or an empty string if `description` is empty/whitespace.
pub fn format_outer_doc(description: &str, indent: &str) -> String {
    format_doc_with_prefix(description, indent, "///")
}

/// Formats a multi-line description string as inner module Rust doc comments (`//!`).
///
/// # Arguments
/// * `description` - The text to format.
/// * `indent` - The leading whitespace indentation prefix for each line.
///
/// # Returns
/// * Formatted module doc comment block ending with a newline, or an empty string if `description` is empty/whitespace.
pub fn format_inner_doc(description: &str, indent: &str) -> String {
    format_doc_with_prefix(description, indent, "//!")
}

/// Executes `format_doc_with_prefix` (format doc with prefix).
///
/// # Arguments
/// * `description` - Input `description` parameter.
/// * `indent` - Input `indent` parameter.
/// * `prefix` - Input `prefix` parameter.
///
/// # Returns
/// Resulting `String` value.
fn format_doc_with_prefix(description: &str, indent: &str, prefix: &str) -> String {
    let trimmed = description.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for line in trimmed.lines() {
        let clean = line.trim_end();
        if clean.is_empty() {
            out.push_str(&format!(
                "{indent}{prefix}
"
            ));
        } else {
            out.push_str(&format!(
                "{indent}{prefix} {clean}
"
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_outer_and_inner_doc() {
        assert_eq!(format_outer_doc("   ", "    "), "");
        assert_eq!(
            format_outer_doc(
                "Summary

Details",
                "    "
            ),
            "    /// Summary
    ///
    /// Details
"
        );
        assert_eq!(
            format_inner_doc("Module title", ""),
            "//! Module title
"
        );
    }
}
