#![deny(missing_docs)]

//! # Arazzo 1.1.0 Intermediate Representation (IR) Models
//!
//! Strongly typed data structures representing the Arazzo Specification 1.1.0
//! (including backward compatibility with 1.0.0 and 1.0.1).

use crate::classes::parse::models::ParsedStruct;
use crate::error::{AppError, AppResult};
use crate::openapi::parse::models::RuntimeExpression;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

/// Top-level Arazzo 1.1.0 Document Object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArazzoDocument {
    /// Semantic version of the Arazzo specification (`1.0.0`, `1.0.1`, `1.1.0`).
    pub arazzo: ArazzoVersion,
    /// Optional `$self` canonical document URI (Arazzo 1.1.0).
    #[serde(rename = "$self", skip_serializing_if = "Option::is_none")]
    pub self_uri: Option<String>,
    /// Metadata about the Arazzo description.
    pub info: ArazzoInfo,
    /// List of source descriptions (OpenAPI, AsyncAPI, or Arazzo documents) referenced by workflows.
    #[serde(rename = "sourceDescriptions")]
    pub source_descriptions: Vec<SourceDescription>,
    /// List of workflows defined in this Arazzo document.
    pub workflows: Vec<Workflow>,
    /// Optional reusable components (`inputs`, `parameters`, `successActions`, `failureActions`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<ArazzoComponents>,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

/// Supported Arazzo Specification versions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArazzoVersion {
    /// Arazzo Specification 1.0.0.
    #[serde(rename = "1.0.0")]
    V1_0_0,
    /// Arazzo Specification 1.0.1.
    #[serde(rename = "1.0.1")]
    V1_0_1,
    /// Arazzo Specification 1.1.0.
    #[serde(rename = "1.1.0")]
    V1_1_0,
    /// Custom patch version within `1.0.x` or `1.1.x`.
    Custom(String),
}

impl ArazzoVersion {
    /// Parses and validates an Arazzo semantic version string (`1.0.x` or `1.1.x`).
    ///
    /// # Arguments
    /// * `raw` - The version string from the `arazzo` root field.
    ///
    /// # Returns
    /// A validated [`ArazzoVersion`].
    ///
    /// # Errors
    /// Returns [`AppError::UnsupportedArazzoVersion`] if the version is not `1.0.x` or `1.1.x`.
    pub fn parse(raw: &str) -> AppResult<Self> {
        let trimmed = raw.trim();
        match trimmed {
            "1.0.0" => Ok(Self::V1_0_0),
            "1.0.1" => Ok(Self::V1_0_1),
            "1.1.0" => Ok(Self::V1_1_0),
            other => {
                if let Some(patch) = other
                    .strip_prefix("1.0.")
                    .or_else(|| other.strip_prefix("1.1."))
                {
                    if !patch.is_empty() && patch.chars().all(|c| c.is_ascii_digit()) {
                        return Ok(Self::Custom(other.to_string()));
                    }
                }
                Err(AppError::UnsupportedArazzoVersion {
                    version: other.to_string(),
                })
            }
        }
    }

    /// Returns the canonical version string slice.
    pub fn as_str(&self) -> &str {
        match self {
            Self::V1_0_0 => "1.0.0",
            Self::V1_0_1 => "1.0.1",
            Self::V1_1_0 => "1.1.0",
            Self::Custom(s) => s.as_str(),
        }
    }
}

impl fmt::Display for ArazzoVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ArazzoVersion {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Metadata about the Arazzo document (`Info Object`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArazzoInfo {
    /// A human-readable title of the Arazzo description.
    pub title: String,
    /// A short summary of the Arazzo description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// A description of the purpose of the workflows defined in the document (CommonMark allowed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The version of the Arazzo document.
    pub version: String,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

/// Type of a [`SourceDescription`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceDescriptionType {
    /// OpenAPI description (`openapi`).
    #[serde(rename = "openapi")]
    OpenApi,
    /// AsyncAPI description (`asyncapi`, Arazzo 1.1.0).
    #[serde(rename = "asyncapi")]
    AsyncApi,
    /// External Arazzo description (`arazzo`).
    #[serde(rename = "arazzo")]
    Arazzo,
}

impl SourceDescriptionType {
    /// Returns the canonical lowercase string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenApi => "openapi",
            Self::AsyncApi => "asyncapi",
            Self::Arazzo => "arazzo",
        }
    }
}

impl fmt::Display for SourceDescriptionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SourceDescriptionType {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "openapi" => Ok(Self::OpenApi),
            "asyncapi" => Ok(Self::AsyncApi),
            "arazzo" => Ok(Self::Arazzo),
            other => Err(AppError::ArazzoValidation {
                path: "sourceDescriptions.type".to_string(),
                reason: format!("Unsupported source description type: '{}'", other),
            }),
        }
    }
}

/// Describes a source specification (such as an OpenAPI, AsyncAPI, or Arazzo document)
/// referenced by one or more workflows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceDescription {
    /// Unique identifier for the source description (`^[A-Za-z0-9_\-]+$`).
    pub name: String,
    /// A URL or relative URI reference to the source description document.
    pub url: String,
    /// The type of the source description (`openapi`, `asyncapi`, `arazzo`).
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub source_type: Option<SourceDescriptionType>,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

/// Strongly typed representation of a JSON Schema 2020-12 input definition in Arazzo
/// (`workflow.inputs` or `components.inputs.<name>`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedSchema {
    /// Parsed Rust struct derived from the JSON Schema object.
    #[serde(skip)]
    pub input_struct: ParsedStruct,
    /// Optional `$ref` or `$components.inputs.<name>` reference if the schema delegates to a component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// Underlying JSON Schema 2020-12 representation for lossless roundtrip emission.
    pub raw_schema: JsonValue,
}

impl ParsedSchema {
    /// Constructs a [`ParsedSchema`] from a raw JSON Schema 2020-12 value and parsed [`ParsedStruct`].
    ///
    /// # Arguments
    /// * `raw_schema` - The JSON Schema 2020-12 value.
    /// * `input_struct` - Extracted [`ParsedStruct`] definition.
    pub fn new(raw_schema: JsonValue, input_struct: ParsedStruct) -> Self {
        let reference = raw_schema
            .get("$ref")
            .and_then(JsonValue::as_str)
            .map(ToOwned::to_owned);
        Self {
            input_struct,
            reference,
            raw_schema,
        }
    }
}

/// Describes the steps to be taken across one or more APIs to achieve an objective (`Workflow Object`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workflow {
    /// Unique identifier for the workflow within the Arazzo document.
    #[serde(rename = "workflowId")]
    pub workflow_id: String,
    /// Short summary of the purpose of the workflow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Description of the workflow (CommonMark allowed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema 2020-12 object defining the input parameters for the workflow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<ParsedSchema>,
    /// List of workflow IDs that MUST be completed before this workflow is processed.
    #[serde(rename = "dependsOn", default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    /// Ordered list of workflow steps.
    pub steps: Vec<WorkflowStep>,
    /// Workflow-level default success actions.
    #[serde(
        rename = "successActions",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub success_actions: Vec<ActionOrReusable<SuccessAction>>,
    /// Workflow-level default failure actions.
    #[serde(
        rename = "failureActions",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub failure_actions: Vec<ActionOrReusable<FailureAction>>,
    /// Map of workflow outputs (`name -> runtime expression or SelectorObject`).
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub outputs: IndexMap<String, RuntimeExpressionOrSelector>,
    /// Workflow-level default parameters shared across steps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ParameterOrReusable>,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

/// Mutually exclusive target of a [`WorkflowStep`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StepTarget {
    /// Reference to an OpenAPI `operationId` (e.g. `"createPet"` or `"$sourceDescriptions.petstore.createPet"`).
    #[serde(rename = "operationId")]
    OperationId(String),
    /// Runtime expression + JSON Pointer to an OpenAPI operation path (e.g. `"{$sourceDescriptions.petstore.url}#/paths/~1pet/post"`).
    #[serde(rename = "operationPath")]
    OperationPath(String),
    /// Reference to an AsyncAPI channel path (Arazzo 1.1.0).
    #[serde(rename = "channelPath")]
    ChannelPath(String),
    /// Reference to a nested or external `workflowId`.
    #[serde(rename = "workflowId")]
    WorkflowId(String),
}

/// AsyncAPI step action direction (Arazzo 1.1.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AsyncActionType {
    /// Publish/send a message (`send`).
    #[serde(rename = "send")]
    Send,
    /// Subscribe/receive a message (`receive`).
    #[serde(rename = "receive")]
    Receive,
}

impl AsyncActionType {
    /// Returns the canonical lowercase string representation (`"send"` or `"receive"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Receive => "receive",
        }
    }
}

impl fmt::Display for AsyncActionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AsyncActionType {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "send" => Ok(Self::Send),
            "receive" => Ok(Self::Receive),
            other => Err(AppError::ArazzoValidation {
                path: "step.action".to_string(),
                reason: format!(
                    "Invalid async step action '{}', expected 'send' or 'receive'",
                    other
                ),
            }),
        }
    }
}

/// A single workflow step operating on an OpenAPI operation, AsyncAPI channel, or sub-workflow (`Step Object`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Unique identifier for the step within the workflow.
    #[serde(rename = "stepId")]
    pub step_id: String,
    /// Optional description of the step (CommonMark allowed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Mutually exclusive target of the step (`operationId`, `operationPath`, `channelPath`, or `workflowId`).
    pub target: StepTarget,
    /// Parameters passed to the target operation or workflow.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ParameterOrReusable>,
    /// Request body to pass to the target operation.
    #[serde(rename = "requestBody", skip_serializing_if = "Option::is_none")]
    pub request_body: Option<ArazzoRequestBody>,
    /// Assertions verified to determine whether the step succeeded.
    #[serde(
        rename = "successCriteria",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub success_criteria: Vec<Criterion>,
    /// Step-level success actions (`end` or `goto`).
    #[serde(rename = "onSuccess", default, skip_serializing_if = "Vec::is_empty")]
    pub on_success: Vec<ActionOrReusable<SuccessAction>>,
    /// Step-level failure actions (`end`, `retry`, or `goto`).
    #[serde(rename = "onFailure", default, skip_serializing_if = "Vec::is_empty")]
    pub on_failure: Vec<ActionOrReusable<FailureAction>>,
    /// Map of step outputs (`name -> runtime expression or SelectorObject`).
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub outputs: IndexMap<String, RuntimeExpressionOrSelector>,
    /// Optional per-step execution timeout in milliseconds (Arazzo 1.1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Optional runtime expression for AsyncAPI message correlation ID (Arazzo 1.1.0).
    #[serde(rename = "correlationId", skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<RuntimeExpression>,
    /// Optional AsyncAPI message action (`send` or `receive`, Arazzo 1.1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<AsyncActionType>,
    /// Prerequisite `stepId` list that MUST complete before this step executes (Arazzo 1.1.0 DAG join).
    #[serde(rename = "dependsOn", default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

/// Location of an [`ArazzoParameter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArazzoParamLocation {
    /// URL path parameter (`path`).
    #[serde(rename = "path")]
    Path,
    /// URL query parameter (`query`).
    #[serde(rename = "query")]
    Query,
    /// Raw URL query string parameter (`querystring`, Arazzo 1.1.0 / OAS 3.2).
    #[serde(rename = "querystring")]
    QueryString,
    /// HTTP header parameter (`header`).
    #[serde(rename = "header")]
    Header,
    /// HTTP cookie parameter (`cookie`).
    #[serde(rename = "cookie")]
    Cookie,
}

impl ArazzoParamLocation {
    /// Returns the canonical lowercase string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Query => "query",
            Self::QueryString => "querystring",
            Self::Header => "header",
            Self::Cookie => "cookie",
        }
    }
}

impl fmt::Display for ArazzoParamLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ArazzoParamLocation {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "path" => Ok(Self::Path),
            "query" => Ok(Self::Query),
            "querystring" => Ok(Self::QueryString),
            "header" => Ok(Self::Header),
            "cookie" => Ok(Self::Cookie),
            other => Err(AppError::ArazzoValidation {
                path: "parameter.in".to_string(),
                reason: format!("Invalid parameter location: '{}'", other),
            }),
        }
    }
}

/// Value passed to an [`ArazzoParameter`], [`ReusableObject`], or [`PayloadReplacement`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterValue {
    /// A validated runtime expression or template (e.g. `"$inputs.petId"`).
    Expression(RuntimeExpression),
    /// A static literal JSON value (number, boolean, object, array, or plain string).
    Literal(JsonValue),
}

impl ParameterValue {
    /// Converts the parameter value into its JSON representation.
    pub fn to_json_value(&self) -> JsonValue {
        match self {
            Self::Expression(expr) => JsonValue::String(expr.as_str().to_string()),
            Self::Literal(val) => val.clone(),
        }
    }
}

/// Describes a single step or workflow parameter (`Parameter Object`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArazzoParameter {
    /// Name of the parameter.
    pub name: String,
    /// Location of the parameter (`path`, `query`, `querystring`, `header`, `cookie`).
    /// Omitted when passing parameters to a sub-workflow.
    #[serde(rename = "in", skip_serializing_if = "Option::is_none")]
    pub in_loc: Option<ArazzoParamLocation>,
    /// Constant literal or runtime expression value.
    pub value: ParameterValue,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

/// A reference to a reusable component in `components` (`Reusable Object`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReusableObject {
    /// Runtime expression pointing to `$components.parameters.<name>`, `$components.successActions.<name>`, or `$components.failureActions.<name>`.
    pub reference: RuntimeExpression,
    /// Optional overridden parameter value (only applicable when referencing `$components.parameters.<name>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<ParameterValue>,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

/// Either an inline [`ArazzoParameter`] or a [`ReusableObject`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterOrReusable {
    /// Inline parameter definition.
    Parameter(ArazzoParameter),
    /// Component reference (`$components.parameters.<name>`).
    Reusable(ReusableObject),
}

/// Either an inline action (`SuccessAction` or `FailureAction`) or a [`ReusableObject`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ActionOrReusable<T> {
    /// Inline action definition.
    Action(T),
    /// Component reference (`$components.successActions.<name>` or `$components.failureActions.<name>`).
    Reusable(ReusableObject),
}

/// Type of a [`SuccessAction`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SuccessActionType {
    /// The workflow ends upon success (`end`).
    #[serde(rename = "end")]
    End,
    /// Control transfers to another step or workflow (`goto`).
    #[serde(rename = "goto")]
    Goto,
}

impl SuccessActionType {
    /// Returns the canonical lowercase string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::End => "end",
            Self::Goto => "goto",
        }
    }
}

impl fmt::Display for SuccessActionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SuccessActionType {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "end" => Ok(Self::End),
            "goto" => Ok(Self::Goto),
            other => Err(AppError::ArazzoValidation {
                path: "successAction.type".to_string(),
                reason: format!("Invalid success action type: '{}'", other),
            }),
        }
    }
}

/// Mutually exclusive target of a `goto` or `retry` action.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ActionTarget {
    /// No target (used when action type is `end` or default self-step `retry`).
    #[default]
    None,
    /// Target another workflow (`workflowId`).
    WorkflowId(String),
    /// Target another step in the current workflow (`stepId`).
    StepId(String),
}

/// Action taken when a step's `successCriteria` are met (`Success Action Object`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuccessAction {
    /// Descriptive name of the success action.
    pub name: String,
    /// Type of action (`end` or `goto`).
    #[serde(rename = "type")]
    pub action_type: SuccessActionType,
    /// Mutually exclusive target (`workflowId` or `stepId`).
    pub target: ActionTarget,
    /// Parameters passed to the target workflow or step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ParameterOrReusable>,
    /// Additional criteria guarding this action.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub criteria: Vec<Criterion>,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

/// Type of a [`FailureAction`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FailureActionType {
    /// Terminate the workflow with failure (`end`).
    #[serde(rename = "end")]
    End,
    /// Retry the step or target (`retry`).
    #[serde(rename = "retry")]
    Retry,
    /// Transfer control to a fallback step or workflow (`goto`).
    #[serde(rename = "goto")]
    Goto,
}

impl FailureActionType {
    /// Returns the canonical lowercase string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::End => "end",
            Self::Retry => "retry",
            Self::Goto => "goto",
        }
    }
}

impl fmt::Display for FailureActionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FailureActionType {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "end" => Ok(Self::End),
            "retry" => Ok(Self::Retry),
            "goto" => Ok(Self::Goto),
            other => Err(AppError::ArazzoValidation {
                path: "failureAction.type".to_string(),
                reason: format!("Invalid failure action type: '{}'", other),
            }),
        }
    }
}

/// Action taken when a step fails (`Failure Action Object`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailureAction {
    /// Descriptive name of the failure action.
    pub name: String,
    /// Type of action (`end`, `retry`, or `goto`).
    #[serde(rename = "type")]
    pub action_type: FailureActionType,
    /// Mutually exclusive target (`workflowId` or `stepId`).
    pub target: ActionTarget,
    /// Parameters passed to the recovery workflow or step.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ParameterOrReusable>,
    /// Non-negative number of seconds to wait before retrying (`retryAfter`).
    #[serde(rename = "retryAfter", skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<f64>,
    /// Maximum number of retry attempts (`retryLimit`).
    #[serde(rename = "retryLimit", skip_serializing_if = "Option::is_none")]
    pub retry_limit: Option<u32>,
    /// Criteria guarding when this failure action applies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub criteria: Vec<Criterion>,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

/// Dialect of a [`CriterionExpressionType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExpressionDialect {
    /// JSONPath (`jsonpath`).
    #[serde(rename = "jsonpath")]
    JsonPath,
    /// XPath (`xpath`).
    #[serde(rename = "xpath")]
    XPath,
    /// JSON Pointer (`jsonpointer`, Arazzo 1.1.0).
    #[serde(rename = "jsonpointer")]
    JsonPointer,
}

impl ExpressionDialect {
    /// Returns the canonical lowercase string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::JsonPath => "jsonpath",
            Self::XPath => "xpath",
            Self::JsonPointer => "jsonpointer",
        }
    }

    /// Validates whether `version` is a supported specification version for this dialect.
    ///
    /// # Arguments
    /// * `version` - Version identifier (e.g. `"draft-goessner-dispatch-jsonpath-00"`, `"rfc9535"`, `"xpath-31"`, `"rfc6901"`).
    ///
    /// # Returns
    /// `Ok(())` if valid.
    ///
    /// # Errors
    /// Returns [`AppError::ArazzoValidation`] if the version does not match the dialect.
    pub fn validate_version(&self, version: &str) -> AppResult<()> {
        let valid = match self {
            Self::JsonPath => {
                matches!(version, "draft-goessner-dispatch-jsonpath-00" | "rfc9535")
            }
            Self::XPath => matches!(version, "xpath-31" | "xpath-20" | "xpath-10"),
            Self::JsonPointer => matches!(version, "rfc6901"),
        };
        if valid {
            Ok(())
        } else {
            Err(AppError::ArazzoValidation {
                path: "criterionExpressionType.version".to_string(),
                reason: format!(
                    "Invalid version '{}' for expression dialect '{}'",
                    version,
                    self.as_str()
                ),
            })
        }
    }
}

impl fmt::Display for ExpressionDialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ExpressionDialect {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "jsonpath" => Ok(Self::JsonPath),
            "xpath" => Ok(Self::XPath),
            "jsonpointer" => Ok(Self::JsonPointer),
            other => Err(AppError::ArazzoValidation {
                path: "criterionExpressionType.type".to_string(),
                reason: format!("Invalid expression dialect: '{}'", other),
            }),
        }
    }
}

/// Detailed expression dialect and version configuration (`Criterion Expression Type Object`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CriterionExpressionType {
    /// Dialect (`jsonpath`, `xpath`, `jsonpointer`).
    #[serde(rename = "type")]
    pub dialect: ExpressionDialect,
    /// Specification version (`draft-goessner-dispatch-jsonpath-00`, `rfc9535`, `xpath-31`, `xpath-20`, `xpath-10`, `rfc6901`).
    pub version: String,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

/// Evaluation type of a [`Criterion`], [`SelectorObject`], or [`PayloadReplacement`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CriterionType {
    /// Simple comparison expression (`simple`, default).
    Simple,
    /// Regular expression evaluation (`regex`).
    Regex,
    /// Default JSONPath evaluation (`jsonpath`).
    JsonPath,
    /// Default XPath evaluation (`xpath`).
    XPath,
    /// Explicit dialect and version object (`CriterionExpressionType`).
    ExpressionType(CriterionExpressionType),
}

impl CriterionType {
    /// Parses a string criterion type (`"simple"`, `"regex"`, `"jsonpath"`, `"xpath"`).
    ///
    /// # Arguments
    /// * `s` - The string representation.
    ///
    /// # Returns
    /// A [`CriterionType`].
    ///
    /// # Errors
    /// Returns [`AppError::ArazzoValidation`] if the string is unknown.
    pub fn from_type_str(s: &str) -> AppResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "simple" => Ok(Self::Simple),
            "regex" => Ok(Self::Regex),
            "jsonpath" => Ok(Self::JsonPath),
            "xpath" => Ok(Self::XPath),
            other => Err(AppError::ArazzoValidation {
                path: "criterion.type".to_string(),
                reason: format!("Unknown criterion type: '{}'", other),
            }),
        }
    }
}

/// Assertion object used in `successCriteria`, `SuccessAction::criteria`, and `FailureAction::criteria`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Criterion {
    /// Optional runtime expression providing the evaluation context (e.g. `"$statusCode"` or `"$response.body"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<RuntimeExpression>,
    /// Condition expression or pattern to evaluate.
    pub condition: String,
    /// Optional criterion evaluation type (defaults to `simple` when `None`).
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub criterion_type: Option<CriterionType>,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

/// Structured output value extractor (`Selector Object`, Arazzo 1.1.0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectorObject {
    /// Runtime expression specifying the document/payload context (e.g. `"$response.body"`).
    pub context: RuntimeExpression,
    /// Selector query string (JSONPath, XPath, or JSON Pointer).
    pub selector: String,
    /// Optional selector dialect/version configuration.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub selector_type: Option<CriterionType>,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

/// Output value definition in `workflow.outputs` or `step.outputs` (either a runtime expression string or a [`SelectorObject`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeExpressionOrSelector {
    /// Direct runtime expression or template string.
    Expression(RuntimeExpression),
    /// Structured Arazzo 1.1.0 [`SelectorObject`].
    Selector(SelectorObject),
}

/// Dynamic mutation applied to a [`ArazzoRequestBody`] payload before transmission (`Payload Replacement Object`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PayloadReplacement {
    /// JSON Pointer, JSONPath, or XPath expression targeting the location inside `payload`.
    pub target: String,
    /// Optional selector type for `target` (Arazzo 1.1.0).
    #[serde(rename = "targetSelectorType", skip_serializing_if = "Option::is_none")]
    pub target_selector_type: Option<CriterionType>,
    /// Replacement value (runtime expression or literal).
    pub value: ParameterValue,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

/// Request body definition for a [`WorkflowStep`] (`RequestBody Object`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArazzoRequestBody {
    /// Optional MIME Content-Type for the payload (e.g. `"application/json"`).
    #[serde(rename = "contentType", skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Static template, literal value, or runtime expression string representing the body payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<JsonValue>,
    /// Dynamic replacements injected into `payload` prior to sending the request.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replacements: Vec<PayloadReplacement>,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

/// Reusable definitions container (`Components Object`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ArazzoComponents {
    /// Reusable JSON Schema 2020-12 input objects (`$components.inputs.<name>`).
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub inputs: IndexMap<String, ParsedSchema>,
    /// Reusable parameter objects (`$components.parameters.<name>`).
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub parameters: IndexMap<String, ArazzoParameter>,
    /// Reusable success action objects (`$components.successActions.<name>`).
    #[serde(
        rename = "successActions",
        default,
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub success_actions: IndexMap<String, SuccessAction>,
    /// Reusable failure action objects (`$components.failureActions.<name>`).
    #[serde(
        rename = "failureActions",
        default,
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub failure_actions: IndexMap<String, FailureAction>,
    /// Specification extensions (`^x-`).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, JsonValue>,
}

impl ArazzoComponents {
    /// Returns `true` if all component registries and extensions are empty.
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
            && self.parameters.is_empty()
            && self.success_actions.is_empty()
            && self.failure_actions.is_empty()
            && self.extensions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_arazzo_version_parse_and_display() {
        assert_eq!(
            ArazzoVersion::parse("1.0.0").expect("1.0.0"),
            ArazzoVersion::V1_0_0
        );
        assert_eq!(
            ArazzoVersion::parse("1.0.1").expect("1.0.1"),
            ArazzoVersion::V1_0_1
        );
        assert_eq!(
            "1.1.0".parse::<ArazzoVersion>().expect("1.1.0"),
            ArazzoVersion::V1_1_0
        );
        let custom = ArazzoVersion::parse("1.1.2").expect("1.1.2");
        assert_eq!(custom, ArazzoVersion::Custom("1.1.2".to_string()));
        assert_eq!(ArazzoVersion::V1_0_0.to_string(), "1.0.0");
        assert_eq!(ArazzoVersion::V1_0_1.to_string(), "1.0.1");
        assert_eq!(ArazzoVersion::V1_1_0.to_string(), "1.1.0");
        assert_eq!(custom.to_string(), "1.1.2");

        assert!(ArazzoVersion::parse("2.0.0").is_err());
        assert!(ArazzoVersion::parse("1.0.").is_err());
        assert!(ArazzoVersion::parse("1.1.x").is_err());
    }

    #[test]
    fn test_source_description_type_and_async_action() {
        for (raw, expected) in [
            ("openapi", SourceDescriptionType::OpenApi),
            ("asyncapi", SourceDescriptionType::AsyncApi),
            ("arazzo", SourceDescriptionType::Arazzo),
        ] {
            let parsed = raw.parse::<SourceDescriptionType>().expect(raw);
            assert_eq!(parsed, expected);
            assert_eq!(parsed.to_string(), raw);
        }
        assert!("unknown".parse::<SourceDescriptionType>().is_err());

        for (raw, expected) in [
            ("send", AsyncActionType::Send),
            ("receive", AsyncActionType::Receive),
        ] {
            let parsed = raw.parse::<AsyncActionType>().expect(raw);
            assert_eq!(parsed, expected);
            assert_eq!(parsed.to_string(), raw);
        }
        assert!("invalid".parse::<AsyncActionType>().is_err());
    }

    #[test]
    fn test_param_location_and_action_types() {
        for (raw, expected) in [
            ("path", ArazzoParamLocation::Path),
            ("query", ArazzoParamLocation::Query),
            ("querystring", ArazzoParamLocation::QueryString),
            ("header", ArazzoParamLocation::Header),
            ("cookie", ArazzoParamLocation::Cookie),
        ] {
            let parsed = raw.parse::<ArazzoParamLocation>().expect(raw);
            assert_eq!(parsed, expected);
            assert_eq!(parsed.to_string(), raw);
        }
        assert!("body".parse::<ArazzoParamLocation>().is_err());

        for (raw, expected) in [
            ("end", SuccessActionType::End),
            ("goto", SuccessActionType::Goto),
        ] {
            let parsed = raw.parse::<SuccessActionType>().expect(raw);
            assert_eq!(parsed, expected);
            assert_eq!(parsed.to_string(), raw);
        }
        assert!("retry".parse::<SuccessActionType>().is_err());

        for (raw, expected) in [
            ("end", FailureActionType::End),
            ("retry", FailureActionType::Retry),
            ("goto", FailureActionType::Goto),
        ] {
            let parsed = raw.parse::<FailureActionType>().expect(raw);
            assert_eq!(parsed, expected);
            assert_eq!(parsed.to_string(), raw);
        }
        assert!("other".parse::<FailureActionType>().is_err());
    }

    #[test]
    fn test_expression_dialect_and_criterion_type() {
        for (raw, dialect, valid_ver) in [
            ("jsonpath", ExpressionDialect::JsonPath, "rfc9535"),
            (
                "jsonpath",
                ExpressionDialect::JsonPath,
                "draft-goessner-dispatch-jsonpath-00",
            ),
            ("xpath", ExpressionDialect::XPath, "xpath-31"),
            ("xpath", ExpressionDialect::XPath, "xpath-20"),
            ("xpath", ExpressionDialect::XPath, "xpath-10"),
            ("jsonpointer", ExpressionDialect::JsonPointer, "rfc6901"),
        ] {
            let parsed = raw.parse::<ExpressionDialect>().expect(raw);
            assert_eq!(parsed, dialect);
            assert_eq!(parsed.to_string(), raw);
            assert!(parsed.validate_version(valid_ver).is_ok());
            assert!(parsed.validate_version("bad-ver").is_err());
        }
        assert!("sql".parse::<ExpressionDialect>().is_err());

        assert_eq!(
            CriterionType::from_type_str("simple").expect("simple"),
            CriterionType::Simple
        );
        assert_eq!(
            CriterionType::from_type_str("regex").expect("regex"),
            CriterionType::Regex
        );
        assert_eq!(
            CriterionType::from_type_str("jsonpath").expect("jsonpath"),
            CriterionType::JsonPath
        );
        assert_eq!(
            CriterionType::from_type_str("xpath").expect("xpath"),
            CriterionType::XPath
        );
        assert!(CriterionType::from_type_str("invalid").is_err());
    }

    #[test]
    fn test_parsed_schema_parameter_value_and_components() {
        let empty_struct = ParsedStruct {
            name: "User".to_string(),
            description: None,
            rename: None,
            rename_all: None,
            fields: vec![],
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
        };
        let schema_ref =
            ParsedSchema::new(json!({"$ref": "#/components/inputs/User"}), empty_struct);
        assert_eq!(
            schema_ref.reference.as_deref(),
            Some("#/components/inputs/User")
        );

        let expr_val = ParameterValue::Expression(
            RuntimeExpression::parse("$url").expect("valid runtime expression"),
        );
        assert_eq!(expr_val.to_json_value(), json!("$url"));
        let lit_val = ParameterValue::Literal(json!(123));
        assert_eq!(lit_val.to_json_value(), json!(123));

        let mut comps = ArazzoComponents::default();
        assert!(comps.is_empty());
        comps.inputs.insert("User".to_string(), schema_ref);
        assert!(!comps.is_empty());
        comps.inputs.clear();
        comps.parameters.insert(
            "p".to_string(),
            ArazzoParameter {
                name: "p".to_string(),
                in_loc: None,
                value: lit_val,
                extensions: BTreeMap::new(),
            },
        );
        assert!(!comps.is_empty());
        comps.parameters.clear();
        comps.success_actions.insert(
            "sa".to_string(),
            SuccessAction {
                name: "sa".to_string(),
                action_type: SuccessActionType::End,
                target: ActionTarget::None,
                parameters: vec![],
                criteria: vec![],
                extensions: BTreeMap::new(),
            },
        );
        assert!(!comps.is_empty());
        comps.success_actions.clear();
        comps.failure_actions.insert(
            "fa".to_string(),
            FailureAction {
                name: "fa".to_string(),
                action_type: FailureActionType::End,
                target: ActionTarget::None,
                parameters: vec![],
                retry_after: None,
                retry_limit: None,
                criteria: vec![],
                extensions: BTreeMap::new(),
            },
        );
        assert!(!comps.is_empty());
        comps.failure_actions.clear();
        comps.extensions.insert("x-ext".to_string(), json!(true));
        assert!(!comps.is_empty());
    }
}
