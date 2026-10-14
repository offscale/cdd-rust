#![deny(missing_docs)]

//! Strongly typed MCP schema models for the Model Context Protocol (2024-11-05).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request ID in JSON-RPC 2.0.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum RequestId {
    /// String ID.
    String(String),
    /// Number ID.
    Number(i64),
    /// Null ID (for notifications or error responses with unknown request ID).
    Null,
}

impl From<i64> for RequestId {
    fn from(n: i64) -> Self {
        Self::Number(n)
    }
}

impl From<&str> for RequestId {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for RequestId {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

/// Progress token for tracking long-running operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ProgressToken {
    /// String token.
    String(String),
    /// Number token.
    Number(i64),
}

/// Logging severity level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LoggingLevel {
    /// Debug level.
    Debug,
    /// Info level.
    Info,
    /// Notice level.
    Notice,
    /// Warning level.
    Warning,
    /// Error level.
    Error,
    /// Critical level.
    Critical,
    /// Alert level.
    Alert,
    /// Emergency level.
    Emergency,
}

/// Conversation participant role.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// User role.
    User,
    /// Assistant role.
    Assistant,
}

/// Represents a JSON-RPC 2.0 Request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonRpcRequest {
    /// The JSON-RPC version, must be `"2.0"`.
    pub jsonrpc: String,
    /// The request ID.
    pub id: RequestId,
    /// The method to be invoked.
    pub method: String,
    /// The method parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// Represents a JSON-RPC 2.0 Response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonRpcResponse {
    /// The JSON-RPC version, must be `"2.0"`.
    pub jsonrpc: String,
    /// The request ID.
    pub id: RequestId,
    /// The result of the method invocation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// The error object in case of failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// Represents a JSON-RPC 2.0 Error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonRpcError {
    /// The error code.
    pub code: i32,
    /// The error message.
    pub message: String,
    /// Additional error data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Represents a JSON-RPC 2.0 Notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonRpcNotification {
    /// The JSON-RPC version, must be `"2.0"`.
    pub jsonrpc: String,
    /// The method to be invoked.
    pub method: String,
    /// The method parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC Message envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    /// Request message.
    Request(JsonRpcRequest),
    /// Response message.
    Response(JsonRpcResponse),
    /// Notification message.
    Notification(JsonRpcNotification),
}

/// Initialize request parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequestParams {
    /// Protocol version.
    pub protocol_version: String,
    /// Client capabilities.
    pub capabilities: ClientCapabilities,
    /// Client info.
    pub client_info: Implementation,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Client capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    /// Experimental capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<Value>,
    /// Roots capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<ClientRootsCapability>,
    /// Sampling capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<Value>,
}

/// Client roots capability.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClientRootsCapability {
    /// Whether the client supports roots list changed notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Implementation details.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Implementation {
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
}

/// Initialize result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// Protocol version.
    pub protocol_version: String,
    /// Server capabilities.
    pub capabilities: ServerCapabilities,
    /// Server info.
    pub server_info: Implementation,
    /// Instructions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Server capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    /// Experimental capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<Value>,
    /// Logging capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<Value>,
    /// Prompts capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<ServerPromptsCapability>,
    /// Resources capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ServerResourcesCapability>,
    /// Tools capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ServerToolsCapability>,
}

/// Server prompts capability.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerPromptsCapability {
    /// Whether the server supports list changed notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Server resources capability.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerResourcesCapability {
    /// Whether the server supports list changed notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
    /// Whether the server supports subscribing to resources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
}

/// Server tools capability.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerToolsCapability {
    /// Whether the server supports list changed notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Tool definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    /// Name.
    pub name: String,
    /// Description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Input schema.
    pub input_schema: ToolInputSchema,
}

/// Tool input schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolInputSchema {
    /// Type (usually `"object"`).
    #[serde(rename = "type")]
    pub schema_type: String,
    /// Properties.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Value>,
    /// Required properties.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

/// Call tool request parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallToolRequestParams {
    /// Name.
    pub name: String,
    /// Arguments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Content returned by a tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum CallToolResultContent {
    /// Text content.
    #[serde(rename = "text")]
    Text(TextContent),
    /// Image content.
    #[serde(rename = "image")]
    Image(ImageContent),
    /// Embedded resource.
    #[serde(rename = "resource")]
    EmbeddedResource(EmbeddedResource),
}

/// Text content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextContent {
    /// Text.
    pub text: String,
}

/// Image content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImageContent {
    /// Data.
    pub data: String,
    /// MIME type.
    pub mime_type: String,
}

/// Embedded resource content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbeddedResource {
    /// Resource.
    pub resource: Value,
    /// Optional annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

/// Call tool result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    /// Content.
    pub content: Vec<CallToolResultContent>,
    /// Is error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List tools request parameters.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ListToolsRequestParams {
    /// Cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List tools result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListToolsResult {
    /// Tools.
    pub tools: Vec<Tool>,
    /// Next cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Resource definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    /// URI.
    pub uri: String,
    /// Name.
    pub name: String,
    /// Description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MIME type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// List resources request parameters.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ListResourcesRequestParams {
    /// Cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List resources result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListResourcesResult {
    /// Resources.
    pub resources: Vec<Resource>,
    /// Next cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Read resource request parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadResourceRequestParams {
    /// URI.
    pub uri: String,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Resource contents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ResourceContents {
    /// Text resource.
    Text(TextResourceContents),
    /// Blob resource.
    Blob(BlobResourceContents),
}

/// Text resource contents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextResourceContents {
    /// URI.
    pub uri: String,
    /// MIME type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Text.
    pub text: String,
}

/// Blob resource contents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BlobResourceContents {
    /// URI.
    pub uri: String,
    /// MIME type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Blob data.
    pub blob: String,
}

/// Read resource result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadResourceResult {
    /// Contents.
    pub contents: Vec<ResourceContents>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Prompt definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    /// Name.
    pub name: String,
    /// Description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Arguments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

/// Prompt argument.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PromptArgument {
    /// Name.
    pub name: String,
    /// Description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// List prompts request parameters.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ListPromptsRequestParams {
    /// Cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List prompts result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListPromptsResult {
    /// Prompts.
    pub prompts: Vec<Prompt>,
    /// Next cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Get prompt request parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetPromptRequestParams {
    /// Name.
    pub name: String,
    /// Arguments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Prompt message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PromptMessage {
    /// Role.
    pub role: Role,
    /// Content.
    pub content: CallToolResultContent,
}

/// Get prompt result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GetPromptResult {
    /// Description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Messages.
    pub messages: Vec<PromptMessage>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Create message request parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateMessageRequestParams {
    /// Messages.
    pub messages: Vec<PromptMessage>,
    /// Model preferences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_preferences: Option<Value>,
    /// System prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Max tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
    /// Stop sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// Include context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_context: Option<String>,
    /// Metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Create message result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreateMessageResult {
    /// Role.
    pub role: Role,
    /// Content.
    pub content: CallToolResultContent,
    /// Model.
    pub model: String,
    /// Stop reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Set logging level request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetLevelRequestParams {
    /// Level.
    pub level: LoggingLevel,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Subscribe request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscribeRequestParams {
    /// URI.
    pub uri: String,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Unsubscribe request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnsubscribeRequestParams {
    /// URI.
    pub uri: String,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Logging message notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LoggingMessageNotificationParams {
    /// Level.
    pub level: LoggingLevel,
    /// Logger.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger: Option<String>,
    /// Data.
    pub data: Value,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Progress notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressNotificationParams {
    /// Progress token.
    pub progress_token: ProgressToken,
    /// Progress.
    pub progress: f64,
    /// Total.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Resource updated notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceUpdatedNotificationParams {
    /// URI.
    pub uri: String,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Ping request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PingRequestParams {}

/// Cancelled notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CancelledNotificationParams {
    /// Request ID.
    pub request_id: RequestId,
    /// Reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List roots request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListRootsRequestParams {}

/// Root.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Root {
    /// URI.
    pub uri: String,
    /// Name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// List roots result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListRootsResult {
    /// Roots.
    pub roots: Vec<Root>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Complete request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompleteRequestParams {
    /// Reference.
    pub ref_: CompleteRequestReference,
    /// Argument.
    pub argument: CompleteRequestArgument,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Complete request reference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum CompleteRequestReference {
    /// Prompt reference.
    #[serde(rename = "ref/prompt")]
    Prompt {
        /// Name.
        name: String,
    },
    /// Resource reference.
    #[serde(rename = "ref/resource")]
    Resource {
        /// URI.
        uri: String,
    },
}

/// Complete request argument.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompleteRequestArgument {
    /// Name.
    pub name: String,
    /// Value.
    pub value: String,
}

/// Complete result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompleteResult {
    /// Completion.
    pub completion: CompleteResultCompletion,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Complete result completion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompleteResultCompletion {
    /// Values.
    pub values: Vec<String>,
    /// Total.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i32>,
    /// Has more.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

/// Sampling message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SamplingMessage {
    /// Role.
    pub role: Role,
    /// Content.
    pub content: CallToolResultContent,
}

/// Empty result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmptyResult {}

/// Resource template.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTemplate {
    /// URI Template.
    pub uri_template: String,
    /// Name.
    pub name: String,
    /// Description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MIME type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// List resource templates request.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ListResourceTemplatesRequestParams {
    /// Cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List resource templates result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListResourceTemplatesResult {
    /// Resource templates.
    pub resource_templates: Vec<ResourceTemplate>,
    /// Next cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Tool list changed notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolListChangedNotificationParams {}

/// Resource list changed notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceListChangedNotificationParams {}

/// Prompt list changed notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptListChangedNotificationParams {}

/// Roots list changed notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootsListChangedNotificationParams {}

/// Annotated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Annotated {
    /// Annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

/// Annotations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Annotations {
    /// Audience.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<String>>,
    /// Priority.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<f64>,
}

/// Client notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ClientNotification {
    /// Initialized.
    Initialized(InitializedNotification),
    /// Cancelled.
    Cancelled(CancelledNotification),
    /// Progress.
    Progress(ProgressNotification),
    /// Roots list changed.
    RootsListChanged(RootsListChangedNotification),
}

/// Initialized notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitializedNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: InitializedNotificationParams,
}

/// Initialized notification params.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitializedNotificationParams {}

/// Cancelled notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CancelledNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: CancelledNotificationParams,
}

/// Progress notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProgressNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: ProgressNotificationParams,
}

/// Roots list changed notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootsListChangedNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: RootsListChangedNotificationParams,
}

/// Server notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ServerNotification {
    /// Cancelled.
    Cancelled(CancelledNotification),
    /// Progress.
    Progress(ProgressNotification),
    /// Resource updated.
    ResourceUpdated(ResourceUpdatedNotification),
    /// Resource list changed.
    ResourceListChanged(ResourceListChangedNotification),
    /// Tool list changed.
    ToolListChanged(ToolListChangedNotification),
    /// Prompt list changed.
    PromptListChanged(PromptListChangedNotification),
    /// Logging message.
    LoggingMessage(LoggingMessageNotification),
}

/// Resource updated notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceUpdatedNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: ResourceUpdatedNotificationParams,
}

/// Resource list changed notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceListChangedNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: ResourceListChangedNotificationParams,
}

/// Tool list changed notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolListChangedNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: ToolListChangedNotificationParams,
}

/// Prompt list changed notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptListChangedNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: PromptListChangedNotificationParams,
}

/// Logging message notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoggingMessageNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: LoggingMessageNotificationParams,
}

/// Model hint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelHint {
    /// Name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Model preferences.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelPreferences {
    /// Hints.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hints: Option<Vec<ModelHint>>,
    /// Cost priority.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_priority: Option<f64>,
    /// Speed priority.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_priority: Option<f64>,
    /// Intelligence priority.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intelligence_priority: Option<f64>,
}

/// Client request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ClientRequest {
    /// Ping.
    Ping(PingRequest),
    /// Initialize.
    Initialize(InitializeRequest),
    /// Complete.
    Complete(CompleteRequest),
    /// Set level.
    SetLevel(SetLevelRequest),
    /// Get prompt.
    GetPrompt(GetPromptRequest),
    /// List prompts.
    ListPrompts(ListPromptsRequest),
    /// List resources.
    ListResources(ListResourcesRequest),
    /// List resource templates.
    ListResourceTemplates(ListResourceTemplatesRequest),
    /// Read resource.
    ReadResource(ReadResourceRequest),
    /// Subscribe.
    Subscribe(SubscribeRequest),
    /// Unsubscribe.
    Unsubscribe(UnsubscribeRequest),
    /// Call tool.
    CallTool(CallToolRequest),
    /// List tools.
    ListTools(ListToolsRequest),
}

/// Server request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ServerRequest {
    /// Ping.
    Ping(PingRequest),
    /// Create message.
    CreateMessage(Box<CreateMessageRequest>),
    /// List roots.
    ListRoots(ListRootsRequest),
}

/// Client result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ClientResult {
    /// Empty.
    Empty(EmptyResult),
    /// Create message.
    CreateMessage(CreateMessageResult),
    /// List roots.
    ListRoots(ListRootsResult),
}

/// Server result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ServerResult {
    /// Empty.
    Empty(EmptyResult),
    /// Initialize.
    Initialize(InitializeResult),
    /// Complete.
    Complete(CompleteResult),
    /// Get prompt.
    GetPrompt(GetPromptResult),
    /// List prompts.
    ListPrompts(ListPromptsResult),
    /// List resources.
    ListResources(ListResourcesResult),
    /// List resource templates.
    ListResourceTemplates(ListResourceTemplatesResult),
    /// Read resource.
    ReadResource(ReadResourceResult),
    /// Call tool.
    CallTool(CallToolResult),
    /// List tools.
    ListTools(ListToolsResult),
}

/// Paginated request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaginatedRequest {
    /// Method.
    pub method: String,
    /// Params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<PaginatedRequestParams>,
}

/// Paginated request params.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaginatedRequestParams {
    /// Cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Paginated result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedResult {
    /// Next cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Ping request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PingRequest {
    /// Method.
    pub method: String,
    /// Params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<PingRequestParams>,
}

/// Initialize request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitializeRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: InitializeRequestParams,
}

/// Complete request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompleteRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: CompleteRequestParams,
}

/// Set level request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetLevelRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: SetLevelRequestParams,
}

/// Get prompt request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetPromptRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: GetPromptRequestParams,
}

/// List prompts request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListPromptsRequest {
    /// Method.
    pub method: String,
    /// Params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<ListPromptsRequestParams>,
}

/// List resources request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListResourcesRequest {
    /// Method.
    pub method: String,
    /// Params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<ListResourcesRequestParams>,
}

/// List resource templates request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListResourceTemplatesRequest {
    /// Method.
    pub method: String,
    /// Params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<ListResourceTemplatesRequestParams>,
}

/// Read resource request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadResourceRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: ReadResourceRequestParams,
}

/// Subscribe request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscribeRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: SubscribeRequestParams,
}

/// Unsubscribe request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnsubscribeRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: UnsubscribeRequestParams,
}

/// Call tool request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallToolRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: CallToolRequestParams,
}

/// List tools request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListToolsRequest {
    /// Method.
    pub method: String,
    /// Params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<ListToolsRequestParams>,
}

/// Create message request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateMessageRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: CreateMessageRequestParams,
}

/// List roots request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListRootsRequest {
    /// Method.
    pub method: String,
    /// Params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<ListRootsRequestParams>,
}

/// Cursor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Cursor {
    /// Next cursor.
    pub next_cursor: String,
}

/// Prompt reference type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PromptReferenceType {
    /// Prompt.
    Prompt,
}

/// Prompt reference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PromptReference {
    /// Type.
    #[serde(rename = "type")]
    pub ref_type: PromptReferenceType,
    /// Name.
    pub name: String,
}

/// Resource reference type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ResourceReferenceType {
    /// Resource.
    Resource,
}

/// Resource reference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceReference {
    /// Type.
    #[serde(rename = "type")]
    pub ref_type: ResourceReferenceType,
    /// URI.
    pub uri: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;
    use serde_json::json;

    fn assert_roundtrip<T>(val: &T)
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let serialized = serde_json::to_string(val).expect("serialization must succeed");
        let deserialized: T =
            serde_json::from_str(&serialized).expect("deserialization must succeed");
        assert_eq!(&deserialized, val);
    }

    #[test]
    fn test_jsonrpc_envelopes_roundtrip() {
        assert_eq!(RequestId::from(1_i64), RequestId::Number(1));
        assert_eq!(RequestId::from("req-1"), RequestId::String("req-1".into()));
        assert_eq!(
            RequestId::from("req-2".to_string()),
            RequestId::String("req-2".into())
        );

        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Number(1),
            method: "test".to_string(),
            params: Some(json!({"foo": "bar"})),
        };
        assert_roundtrip(&req);

        let res = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: RequestId::String("id-1".to_string()),
            result: Some(json!({"foo": "bar"})),
            error: None,
        };
        assert_roundtrip(&res);

        let err = JsonRpcError {
            code: -32600,
            message: "Invalid Request".to_string(),
            data: Some(json!({"detail": "missing method"})),
        };
        assert_roundtrip(&err);

        let notif = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "notifications/initialized".to_string(),
            params: None,
        };
        assert_roundtrip(&notif);

        assert_roundtrip(&JsonRpcMessage::Request(req));
        assert_roundtrip(&JsonRpcMessage::Response(res));
        assert_roundtrip(&JsonRpcMessage::Notification(notif));
    }

    #[test]
    fn test_initialization_and_tools_roundtrip() {
        let init_req = InitializeRequestParams {
            meta: None,
            protocol_version: "2024-11-05".to_string(),
            capabilities: ClientCapabilities {
                experimental: None,
                roots: Some(ClientRootsCapability {
                    list_changed: Some(true),
                }),
                sampling: None,
            },
            client_info: Implementation {
                name: "test-client".to_string(),
                version: "1.0.0".to_string(),
            },
        };
        assert_roundtrip(&init_req);

        let init_res = InitializeResult {
            meta: None,
            protocol_version: "2024-11-05".to_string(),
            capabilities: ServerCapabilities {
                experimental: None,
                logging: None,
                prompts: Some(ServerPromptsCapability {
                    list_changed: Some(false),
                }),
                resources: Some(ServerResourcesCapability {
                    list_changed: Some(true),
                    subscribe: Some(true),
                }),
                tools: Some(ServerToolsCapability {
                    list_changed: Some(true),
                }),
            },
            server_info: Implementation {
                name: "test-server".to_string(),
                version: "1.0.0".to_string(),
            },
            instructions: Some("Use tools carefully".to_string()),
        };
        assert_roundtrip(&init_res);

        let tool = Tool {
            name: "my_tool".to_string(),
            description: Some("A tool".to_string()),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(json!({"arg": {"type": "string"}})),
                required: Some(vec!["arg".to_string()]),
            },
        };
        assert_roundtrip(&tool);

        let call_req = CallToolRequestParams {
            meta: None,
            name: "my_tool".to_string(),
            arguments: Some(json!({"arg": "val"})),
        };
        assert_roundtrip(&call_req);

        let call_res = CallToolResult {
            meta: None,
            content: vec![
                CallToolResultContent::Text(TextContent {
                    text: "output".to_string(),
                }),
                CallToolResultContent::Image(ImageContent {
                    data: "base64data".to_string(),
                    mime_type: "image/png".to_string(),
                }),
                CallToolResultContent::EmbeddedResource(EmbeddedResource {
                    resource: json!({"uri": "file:///a"}),
                    annotations: None,
                }),
            ],
            is_error: Some(false),
        };
        assert_roundtrip(&call_res);

        assert_roundtrip(&ListToolsRequestParams::default());
        assert_roundtrip(&ListToolsResult {
            meta: None,
            tools: vec![tool],
            next_cursor: None,
        });
    }

    #[test]
    fn test_resources_and_prompts_roundtrip() {
        let resource = Resource {
            uri: "file:///a".to_string(),
            name: "a".to_string(),
            description: Some("file a".to_string()),
            mime_type: Some("text/plain".to_string()),
        };
        assert_roundtrip(&resource);

        assert_roundtrip(&ListResourcesRequestParams::default());
        assert_roundtrip(&ListResourcesResult {
            resources: vec![resource],
            next_cursor: None,
            meta: None,
        });

        assert_roundtrip(&ReadResourceRequestParams {
            uri: "file:///a".to_string(),
            meta: None,
        });

        assert_roundtrip(&ReadResourceResult {
            meta: None,
            contents: vec![
                ResourceContents::Text(TextResourceContents {
                    uri: "file:///a".to_string(),
                    mime_type: None,
                    text: "hello".to_string(),
                }),
                ResourceContents::Blob(BlobResourceContents {
                    uri: "file:///b".to_string(),
                    mime_type: None,
                    blob: "YmxvYg==".to_string(),
                }),
            ],
        });

        let prompt = Prompt {
            name: "code_review".to_string(),
            description: None,
            arguments: Some(vec![PromptArgument {
                name: "code".to_string(),
                description: None,
                required: Some(true),
            }]),
        };
        assert_roundtrip(&prompt);
        assert_roundtrip(&ListPromptsRequestParams::default());
        assert_roundtrip(&ListPromptsResult {
            prompts: vec![prompt],
            next_cursor: None,
            meta: None,
        });
        assert_roundtrip(&GetPromptRequestParams {
            name: "code_review".to_string(),
            arguments: None,
            meta: None,
        });
        assert_roundtrip(&GetPromptResult {
            description: None,
            messages: vec![PromptMessage {
                role: Role::User,
                content: CallToolResultContent::Text(TextContent {
                    text: "review this".to_string(),
                }),
            }],
            meta: None,
        });
    }

    #[test]
    fn test_messages_logging_and_notifications_roundtrip() {
        assert_roundtrip(&CreateMessageRequestParams {
            messages: vec![PromptMessage {
                role: Role::User,
                content: CallToolResultContent::Text(TextContent {
                    text: "hi".to_string(),
                }),
            }],
            model_preferences: None,
            system_prompt: None,
            temperature: Some(0.5),
            max_tokens: Some(100),
            stop_sequences: None,
            include_context: None,
            metadata: None,
            meta: None,
        });

        assert_roundtrip(&CreateMessageResult {
            role: Role::Assistant,
            content: CallToolResultContent::Text(TextContent {
                text: "hello".to_string(),
            }),
            model: "gemini".to_string(),
            stop_reason: Some("endTurn".to_string()),
            meta: None,
        });

        assert_roundtrip(&SetLevelRequestParams {
            level: LoggingLevel::Info,
            meta: None,
        });
        assert_roundtrip(&SubscribeRequestParams {
            uri: "file:///a".to_string(),
            meta: None,
        });
        assert_roundtrip(&UnsubscribeRequestParams {
            uri: "file:///a".to_string(),
            meta: None,
        });
        assert_roundtrip(&LoggingMessageNotificationParams {
            level: LoggingLevel::Warning,
            logger: Some("auth".to_string()),
            data: json!("warn"),
            meta: None,
        });
        assert_roundtrip(&ProgressNotificationParams {
            progress_token: ProgressToken::Number(42),
            progress: 0.5,
            total: Some(1.0),
            meta: None,
        });
        assert_roundtrip(&ProgressNotificationParams {
            progress_token: ProgressToken::String("tok".to_string()),
            progress: 1.0,
            total: None,
            meta: None,
        });
        assert_roundtrip(&ResourceUpdatedNotificationParams {
            uri: "file:///a".to_string(),
            meta: None,
        });
        assert_roundtrip(&PingRequestParams {});
        assert_roundtrip(&CancelledNotificationParams {
            request_id: RequestId::Number(7),
            reason: Some("timeout".to_string()),
            meta: None,
        });
        assert_roundtrip(&ListRootsRequestParams {});
        assert_roundtrip(&ListRootsResult {
            roots: vec![Root {
                uri: "file:///workspace".to_string(),
                name: Some("ws".to_string()),
            }],
            meta: None,
        });
        assert_roundtrip(&CompleteRequestParams {
            ref_: CompleteRequestReference::Prompt {
                name: "p".to_string(),
            },
            argument: CompleteRequestArgument {
                name: "a".to_string(),
                value: "v".to_string(),
            },
            meta: None,
        });
        assert_roundtrip(&CompleteRequestParams {
            ref_: CompleteRequestReference::Resource {
                uri: "file:///r".to_string(),
            },
            argument: CompleteRequestArgument {
                name: "a".to_string(),
                value: "v".to_string(),
            },
            meta: None,
        });
        assert_roundtrip(&CompleteResult {
            completion: CompleteResultCompletion {
                values: vec!["v1".to_string()],
                total: Some(1),
                has_more: Some(false),
            },
            meta: None,
        });
        assert_roundtrip(&SamplingMessage {
            role: Role::User,
            content: CallToolResultContent::Text(TextContent {
                text: "sample".to_string(),
            }),
        });
        assert_roundtrip(&EmptyResult {});
        assert_roundtrip(&ResourceTemplate {
            uri_template: "file:///{path}".to_string(),
            name: "tmpl".to_string(),
            description: None,
            mime_type: None,
        });
        assert_roundtrip(&ListResourceTemplatesRequestParams::default());
        assert_roundtrip(&ListResourceTemplatesResult {
            resource_templates: vec![],
            next_cursor: None,
            meta: None,
        });
    }

    #[test]
    fn test_wrappers_and_references_roundtrip() {
        assert_roundtrip(&ToolListChangedNotificationParams {});
        assert_roundtrip(&ResourceListChangedNotificationParams {});
        assert_roundtrip(&PromptListChangedNotificationParams {});
        assert_roundtrip(&RootsListChangedNotificationParams {});
        assert_roundtrip(&Annotated {
            annotations: Some(Annotations {
                audience: Some(vec!["user".to_string()]),
                priority: Some(0.8),
            }),
        });
        assert_roundtrip(&ClientNotification::Initialized(InitializedNotification {
            method: "notifications/initialized".to_string(),
            params: InitializedNotificationParams {},
        }));
        assert_roundtrip(&ServerNotification::ResourceUpdated(
            ResourceUpdatedNotification {
                method: "notifications/resources/updated".to_string(),
                params: ResourceUpdatedNotificationParams {
                    uri: "file:///a".to_string(),
                    meta: None,
                },
            },
        ));
        assert_roundtrip(&ToolListChangedNotification {
            method: "notifications/tools/list_changed".to_string(),
            params: ToolListChangedNotificationParams {},
        });
        assert_roundtrip(&ModelPreferences {
            hints: Some(vec![ModelHint {
                name: Some("pro".to_string()),
            }]),
            cost_priority: Some(0.2),
            speed_priority: Some(0.5),
            intelligence_priority: Some(0.9),
        });
        assert_roundtrip(&ClientRequest::Ping(PingRequest {
            method: "ping".to_string(),
            params: None,
        }));
        assert_roundtrip(&ServerRequest::Ping(PingRequest {
            method: "ping".to_string(),
            params: None,
        }));
        assert_roundtrip(&ClientResult::Empty(EmptyResult {}));
        assert_roundtrip(&ServerResult::Empty(EmptyResult {}));
        assert_roundtrip(&PaginatedRequest {
            method: "tools/list".to_string(),
            params: Some(PaginatedRequestParams {
                cursor: Some("c1".to_string()),
            }),
        });
        assert_roundtrip(&PaginatedResult {
            next_cursor: Some("c2".to_string()),
            meta: None,
        });
        assert_roundtrip(&Cursor {
            next_cursor: "c3".to_string(),
        });
        assert_roundtrip(&PromptReference {
            ref_type: PromptReferenceType::Prompt,
            name: "p".to_string(),
        });
        assert_roundtrip(&ResourceReference {
            ref_type: ResourceReferenceType::Resource,
            uri: "file:///a".to_string(),
        });
    }
}
