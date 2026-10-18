#![deny(missing_docs)]

//! # Error Handling
//!
//! Provides the unified `AppError` enum used across the workspace.

use derive_more::{Display, Error, From};

/// The Global Error Enum used across `cdd-core`, `cdd-cli`, and `cdd-web`.
///
/// Uses `derive_more` for `Display`, `Error`, and `From` conversions.
#[derive(Debug, Display, Error, From)]
pub enum AppError {
    /// Wrapper for standard IO errors.
    #[display("IO Error: {_0}")]
    Io(std::io::Error),

    /// Wrapper for string formatting errors.
    #[display("Format Error: {_0}")]
    Fmt(std::fmt::Error),

    /// Wrapper for JSON serialization and deserialization errors.
    #[display("JSON Error: {_0}")]
    SerdeJson(serde_json::Error),

    /// Wrapper for YAML serialization and deserialization errors.
    #[display("YAML Error: {_0}")]
    SerdeYaml(serde_yaml::Error),

    /// Wrapper for URL parsing errors.
    #[display("URL Parse Error: {_0}")]
    UrlParse(url::ParseError),

    /// Wrapper for UTF-8 decoding errors.
    #[display("UTF-8 Error: {_0}")]
    Utf8(std::str::Utf8Error),

    /// AST parsing failure when analyzing Rust source code.
    #[display("AST Parse Error in {target}: {details}")]
    #[from(ignore)]
    #[error(ignore)]
    AstParse {
        /// The AST element or file target being parsed.
        target: String,
        /// Detailed explanation of the syntax/structure failure.
        details: String,
    },

    /// Schema validation failure when inspecting OpenAPI or JSON Schema definitions.
    #[display("Schema Validation Error at {path}: {reason}")]
    #[from(ignore)]
    #[error(ignore)]
    SchemaValidation {
        /// The JSON Pointer or document path where validation failed.
        path: String,
        /// Explanation of the validation failure.
        reason: String,
    },

    /// Reference (`$ref`) resolution failure across internal or external documents.
    #[display("Reference Resolution Error for '{reference}': {reason}")]
    #[from(ignore)]
    #[error(ignore)]
    ReferenceResolution {
        /// The `$ref` URI or pointer string.
        reference: String,
        /// Explanation of why resolution failed.
        reason: String,
    },

    /// Cyclic reference detected in schemas, parameters, headers, links, or path items.
    #[display("Cycle Detected in {kind}: {target}")]
    #[from(ignore)]
    #[error(ignore)]
    CycleDetected {
        /// The category of OpenAPI object (e.g. "schema", "parameter", "pathItem").
        kind: String,
        /// The identifier or reference involved in the cycle.
        target: String,
    },

    /// Unsupported OpenAPI or Swagger specification version.
    #[display("Unsupported Specification Version: {version}")]
    #[from(ignore)]
    #[error(ignore)]
    UnsupportedSpecVersion {
        /// The unsupported version string encountered.
        version: String,
    },

    /// Missing required field in a specification or configuration object.
    #[display("Missing Required Field '{field}' in {container}")]
    #[from(ignore)]
    #[error(ignore)]
    MissingRequiredField {
        /// The parent object or struct name.
        container: String,
        /// The missing field name.
        field: String,
    },

    /// Schema or structural validation failure when inspecting Arazzo 1.1.0 definitions.
    #[display("Arazzo Validation Error at {path}: {reason}")]
    #[from(ignore)]
    #[error(ignore)]
    ArazzoValidation {
        /// The JSON Pointer or document path where validation failed.
        path: String,
        /// Explanation of the validation failure.
        reason: String,
    },

    /// Unsupported Arazzo specification version.
    #[display("Unsupported Arazzo Specification Version: {version}")]
    #[from(ignore)]
    #[error(ignore)]
    UnsupportedArazzoVersion {
        /// The unsupported version string encountered.
        version: String,
    },

    /// Cyclic dependency detected in workflow or step `dependsOn` graph.
    #[display("Workflow Cycle Detected in '{workflow_id}': {chain}")]
    #[from(ignore)]
    #[error(ignore)]
    WorkflowCycleDetected {
        /// The workflow ID where the cycle was detected.
        workflow_id: String,
        /// The dependency chain illustrating the cycle.
        chain: String,
    },

    /// Runtime expression syntax or ABNF validation error.
    #[display("Runtime Expression Parse Error in '{expression}': {reason}")]
    #[from(ignore)]
    #[error(ignore)]
    RuntimeExpressionParse {
        /// The invalid runtime expression string.
        expression: String,
        /// Detailed explanation of the syntax error.
        reason: String,
    },

    /// Failure resolving an Arazzo `sourceDescriptions` entry.
    #[display("Source Description Resolution Error for '{name}' ({url}): {reason}")]
    #[from(ignore)]
    #[error(ignore)]
    SourceDescriptionResolution {
        /// The logical name of the source description.
        name: String,
        /// The URL or path of the source description.
        url: String,
        /// Explanation of why resolution failed.
        reason: String,
    },

    /// Wrapper for database or schema synchronization errors.
    #[from(ignore)]
    #[error(ignore)]
    #[display("Database Error: {_0}")]
    Database(String),

    /// Generic application error message.
    #[error(ignore)]
    #[display("General Error: {_0}")]
    General(String),
}

impl From<&str> for AppError {
    /// Converts a static or borrowed string slice into `AppError::General`.
    ///
    /// # Arguments
    /// * `s` - The error message slice.
    ///
    /// # Returns
    /// * `AppError::General` containing the owned string.
    fn from(s: &str) -> Self {
        AppError::General(s.to_string())
    }
}

/// Helper type alias for `Result` using [`AppError`].
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_and_conversions() {
        let err: AppError = "some error".into();
        assert_eq!(format!("{err}"), "General Error: some error");

        let ast_err = AppError::AstParse {
            target: "User".to_string(),
            details: "missing brace".to_string(),
        };
        assert_eq!(
            format!("{ast_err}"),
            "AST Parse Error in User: missing brace"
        );

        let schema_err = AppError::SchemaValidation {
            path: "/paths/pets".to_string(),
            reason: "invalid type".to_string(),
        };
        assert_eq!(
            format!("{schema_err}"),
            "Schema Validation Error at /paths/pets: invalid type"
        );

        let ref_err = AppError::ReferenceResolution {
            reference: "#/components/schemas/Missing".to_string(),
            reason: "not found".to_string(),
        };
        assert_eq!(
            format!("{ref_err}"),
            "Reference Resolution Error for '#/components/schemas/Missing': not found"
        );

        let cycle_err = AppError::CycleDetected {
            kind: "schema".to_string(),
            target: "Node".to_string(),
        };
        assert_eq!(format!("{cycle_err}"), "Cycle Detected in schema: Node");

        let ver_err = AppError::UnsupportedSpecVersion {
            version: "1.0.0".to_string(),
        };
        assert_eq!(
            format!("{ver_err}"),
            "Unsupported Specification Version: 1.0.0"
        );

        let missing_err = AppError::MissingRequiredField {
            container: "Info".to_string(),
            field: "title".to_string(),
        };
        assert_eq!(
            format!("{missing_err}"),
            "Missing Required Field 'title' in Info"
        );

        let arazzo_val_err = AppError::ArazzoValidation {
            path: "/workflows/0".to_string(),
            reason: "empty steps".to_string(),
        };
        assert_eq!(
            format!("{arazzo_val_err}"),
            "Arazzo Validation Error at /workflows/0: empty steps"
        );

        let arazzo_ver_err = AppError::UnsupportedArazzoVersion {
            version: "2.0.0".to_string(),
        };
        assert_eq!(
            format!("{arazzo_ver_err}"),
            "Unsupported Arazzo Specification Version: 2.0.0"
        );

        let wf_cycle_err = AppError::WorkflowCycleDetected {
            workflow_id: "wf1".to_string(),
            chain: "wf1 -> wf2 -> wf1".to_string(),
        };
        assert_eq!(
            format!("{wf_cycle_err}"),
            "Workflow Cycle Detected in 'wf1': wf1 -> wf2 -> wf1"
        );

        let expr_err = AppError::RuntimeExpressionParse {
            expression: "$invalid".to_string(),
            reason: "unknown root".to_string(),
        };
        assert_eq!(
            format!("{expr_err}"),
            "Runtime Expression Parse Error in '$invalid': unknown root"
        );

        let src_err = AppError::SourceDescriptionResolution {
            name: "petstore".to_string(),
            url: "./petstore.yaml".to_string(),
            reason: "file not found".to_string(),
        };
        assert_eq!(
            format!("{src_err}"),
            "Source Description Resolution Error for 'petstore' (./petstore.yaml): file not found"
        );

        let db_err = AppError::Database("connection lost".to_string());
        assert_eq!(format!("{db_err}"), "Database Error: connection lost");
    }
}
