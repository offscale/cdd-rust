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

    /// Missing required CLI, MCP, or JSON-RPC argument.
    #[display("Missing required argument: '{argument}'")]
    #[from(ignore)]
    #[error(ignore)]
    MissingArgument {
        /// The name of the missing required argument.
        argument: &'static str,
    },

    /// Requested MCP or JSON-RPC tool does not exist.
    #[display("Tool not found: {tool}")]
    #[from(ignore)]
    #[error(ignore)]
    ToolNotFound {
        /// The unrecognized tool name.
        tool: String,
    },

    /// Invalid or unsupported specification version for a known standard.
    #[display("Invalid {spec} Specification Version: {version}")]
    #[from(ignore)]
    #[error(ignore)]
    InvalidSpecVersion {
        /// The specification standard name (e.g., `"OpenAPI"`, `"Arazzo"`).
        spec: &'static str,
        /// The invalid version string encountered.
        version: String,
    },

    /// Duplicate identifier encountered where uniqueness is required.
    #[display("Duplicate {kind} identifier: '{id}'")]
    #[from(ignore)]
    #[error(ignore)]
    DuplicateIdentifier {
        /// The kind of identifier (e.g., `"operationId"`, `"workflowId"`, `"stepId"`).
        kind: &'static str,
        /// The duplicated identifier value.
        id: String,
    },

    /// Unresolved reference within a specific document or component scope.
    #[display("Unresolved Reference '{reference}' in {context}")]
    #[from(ignore)]
    #[error(ignore)]
    UnresolvedReference {
        /// The reference string that could not be resolved.
        reference: String,
        /// The document or component context where resolution was attempted.
        context: String,
    },

    /// AST or CST mutation failure when patching or emitting Rust code.
    #[display("AST Mutation Error in {target}: {reason}")]
    #[from(ignore)]
    #[error(ignore)]
    AstMutationError {
        /// The AST node, function, or module being mutated.
        target: String,
        /// Explanation of why the mutation could not be applied.
        reason: String,
    },

    /// Missing configuration or invalid runtime setup.
    #[from(ignore)]
    #[error(ignore)]
    #[display("Configuration Error: {_0}")]
    ConfigError(String),

    /// Operation or handler functionality is not implemented (e.g., stub mode).
    #[display("Not implemented")]
    NotImplemented,

    /// Requested resource or record was not found.
    #[from(ignore)]
    #[error(ignore)]
    #[display("Not Found: {_0}")]
    NotFound(String),

    /// Threading or mutex synchronization failure.
    #[from(ignore)]
    #[error(ignore)]
    #[display("Synchronization Error: {_0}")]
    SyncError(String),

    /// Authentication or authorization failure.
    #[from(ignore)]
    #[error(ignore)]
    #[display("Unauthorized: {_0}")]
    Unauthorized(String),

    /// Request or parameter validation failure.
    #[from(ignore)]
    #[error(ignore)]
    #[display("Validation Error: {_0}")]
    ValidationError(String),

    /// Wrapper for database or schema synchronization errors.
    #[from(ignore)]
    #[error(ignore)]
    #[display("Database Error: {_0}")]
    Database(String),

    /// Generic application error message.
    #[from(ignore)]
    #[error(ignore)]
    #[display("General Error: {_0}")]
    General(String),
}

impl From<String> for AppError {
    /// Converts an owned `String` into `AppError::General`.
    ///
    /// # Arguments
    /// * `s` - The owned error message string.
    ///
    /// # Returns
    /// * `AppError::General` containing `s`.
    fn from(s: String) -> Self {
        AppError::General(s)
    }
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
        let owned_err: AppError = String::from("owned error").into();
        assert_eq!(format!("{owned_err}"), "General Error: owned error");

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

        let missing_arg = AppError::MissingArgument { argument: "input" };
        assert_eq!(
            format!("{missing_arg}"),
            "Missing required argument: 'input'"
        );

        let tool_err = AppError::ToolNotFound {
            tool: "unknown_tool".to_string(),
        };
        assert_eq!(format!("{tool_err}"), "Tool not found: unknown_tool");

        let inv_spec = AppError::InvalidSpecVersion {
            spec: "OpenAPI",
            version: "2.9.0".to_string(),
        };
        assert_eq!(
            format!("{inv_spec}"),
            "Invalid OpenAPI Specification Version: 2.9.0"
        );

        let dup_id = AppError::DuplicateIdentifier {
            kind: "operationId",
            id: "getPets".to_string(),
        };
        assert_eq!(
            format!("{dup_id}"),
            "Duplicate operationId identifier: 'getPets'"
        );

        let unres_ref = AppError::UnresolvedReference {
            reference: "#/components/schemas/Pet".to_string(),
            context: "paths./pets.get".to_string(),
        };
        assert_eq!(
            format!("{unres_ref}"),
            "Unresolved Reference '#/components/schemas/Pet' in paths./pets.get"
        );

        let ast_mut = AppError::AstMutationError {
            target: "config".to_string(),
            reason: "missing body".to_string(),
        };
        assert_eq!(
            format!("{ast_mut}"),
            "AST Mutation Error in config: missing body"
        );

        let cfg_err = AppError::ConfigError("missing secret".to_string());
        assert_eq!(format!("{cfg_err}"), "Configuration Error: missing secret");

        let not_impl = AppError::NotImplemented;
        assert_eq!(format!("{not_impl}"), "Not implemented");

        let not_found = AppError::NotFound("user 123".to_string());
        assert_eq!(format!("{not_found}"), "Not Found: user 123");

        let sync_err = AppError::SyncError("lock poisoned".to_string());
        assert_eq!(
            format!("{sync_err}"),
            "Synchronization Error: lock poisoned"
        );

        let unauth = AppError::Unauthorized("expired token".to_string());
        assert_eq!(format!("{unauth}"), "Unauthorized: expired token");

        let val_err = AppError::ValidationError("invalid email".to_string());
        assert_eq!(format!("{val_err}"), "Validation Error: invalid email");

        let db_err = AppError::Database("connection lost".to_string());
        assert_eq!(format!("{db_err}"), "Database Error: connection lost");
    }
}
