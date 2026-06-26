#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! Strongly typed MCP schema models.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Represents a JSON-RPC 2.0 Request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// The JSON-RPC version, must be "2.0".
    pub jsonrpc: String,
    /// The request ID.
    pub id: Value,
    /// The method to be invoked.
    pub method: String,
    /// The method parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// Represents a JSON-RPC 2.0 Response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// The JSON-RPC version, must be "2.0".
    pub jsonrpc: String,
    /// The request ID.
    pub id: Value,
    /// The result of the method invocation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// The error object in case of failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// Represents a JSON-RPC 2.0 Error.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    /// The JSON-RPC version, must be "2.0".
    pub jsonrpc: String,
    /// The method to be invoked.
    pub method: String,
    /// The method parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// Initialize request parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClientRootsCapability {
    /// Whether the client supports roots list changed notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Implementation details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Implementation {
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
}

/// Initialize result.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerPromptsCapability {
    /// Whether the server supports list changed notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Server resources capability.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerToolsCapability {
    /// Whether the server supports list changed notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInputSchema {
    /// Type (usually "object").
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextContent {
    /// Text.
    pub text: String,
}

/// Image content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageContent {
    /// Data.
    pub data: String,
    /// MIME type.
    pub mime_type: String,
}

/// Embedded resource content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedResource {
    /// Type of content, must be "resource".
    #[serde(rename = "type")]
    pub type_: String,
    /// Resource.
    pub resource: Value,
    /// Optional annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

/// Call tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListToolsRequestParams {
    /// Cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List tools result.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_jsonrpc_request_serialization() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(1),
            method: "test".to_string(),
            params: Some(json!({"foo": "bar"})),
        };
        let serialized = serde_json::to_string(&req).expect("must succeed");
        assert!(serialized.contains("test"));

        let deserialized: JsonRpcRequest = serde_json::from_str(&serialized).expect("must succeed");
        assert_eq!(deserialized.method, "test");
    }

    #[test]
    fn test_jsonrpc_response_serialization() {
        let res = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: json!(1),
            result: Some(json!({"foo": "bar"})),
            error: None,
        };
        let serialized = serde_json::to_string(&res).expect("must succeed");
        assert!(serialized.contains("bar"));

        let deserialized: JsonRpcResponse =
            serde_json::from_str(&serialized).expect("must succeed");
        assert!(deserialized.result.is_some());
    }

    #[test]
    fn test_jsonrpc_error_serialization() {
        let err = JsonRpcError {
            code: 1,
            message: "error".to_string(),
            data: None,
        };
        let serialized = serde_json::to_string(&err).expect("must succeed");
        assert!(serialized.contains("error"));

        let deserialized: JsonRpcError = serde_json::from_str(&serialized).expect("must succeed");
        assert_eq!(deserialized.message, "error");
    }

    #[test]
    fn test_jsonrpc_notification_serialization() {
        let notif = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "notif".to_string(),
            params: None,
        };
        let serialized = serde_json::to_string(&notif).expect("must succeed");
        assert!(serialized.contains("notif"));

        let deserialized: JsonRpcNotification =
            serde_json::from_str(&serialized).expect("must succeed");
        assert_eq!(deserialized.method, "notif");
    }

    #[test]
    fn test_initialize_request_serialization() {
        let req = InitializeRequestParams {
            meta: None,
            protocol_version: "2024-11-05".to_string(),
            capabilities: ClientCapabilities::default(),
            client_info: Implementation {
                name: "test".to_string(),
                version: "1.0".to_string(),
            },
        };
        let serialized = serde_json::to_string(&req).expect("must succeed");
        assert!(serialized.contains("2024-11-05"));

        let deserialized: InitializeRequestParams =
            serde_json::from_str(&serialized).expect("must succeed");
        assert_eq!(deserialized.protocol_version, "2024-11-05");
    }

    #[test]
    fn test_initialize_result_serialization() {
        let res = InitializeResult {
            meta: None,
            protocol_version: "2024-11-05".to_string(),
            capabilities: ServerCapabilities::default(),
            server_info: Implementation {
                name: "test".to_string(),
                version: "1.0".to_string(),
            },
            instructions: None,
        };
        let serialized = serde_json::to_string(&res).expect("must succeed");
        assert!(serialized.contains("2024-11-05"));

        let deserialized: InitializeResult =
            serde_json::from_str(&serialized).expect("must succeed");
        assert_eq!(deserialized.protocol_version, "2024-11-05");
    }

    #[test]
    fn test_tool_serialization() {
        let tool = Tool {
            name: "test".to_string(),
            description: None,
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: None,
                required: None,
            },
        };
        let serialized = serde_json::to_string(&tool).expect("must succeed");
        assert!(serialized.contains("test"));

        let deserialized: Tool = serde_json::from_str(&serialized).expect("must succeed");
        assert_eq!(deserialized.name, "test");
    }

    #[test]
    fn test_call_tool_request_serialization() {
        let req = CallToolRequestParams {
            meta: None,
            name: "test".to_string(),
            arguments: None,
        };
        let serialized = serde_json::to_string(&req).expect("must succeed");
        assert!(serialized.contains("test"));

        let deserialized: CallToolRequestParams =
            serde_json::from_str(&serialized).expect("must succeed");
        assert_eq!(deserialized.name, "test");
    }

    #[test]
    fn test_call_tool_result_serialization() {
        let res = CallToolResult {
            meta: None,
            content: vec![CallToolResultContent::Text(TextContent {
                text: "test".to_string(),
            })],
            is_error: None,
        };
        let serialized = serde_json::to_string(&res).expect("must succeed");
        assert!(serialized.contains("test"));

        let deserialized: CallToolResult = serde_json::from_str(&serialized).expect("must succeed");
        assert_eq!(deserialized.content.len(), 1);
    }

    #[test]
    fn test_call_tool_result_content_variants() {
        let image = CallToolResultContent::Image(ImageContent {
            data: "data".to_string(),
            mime_type: "image/png".to_string(),
        });
        let serialized = serde_json::to_string(&image).expect("must succeed");
        assert!(serialized.contains("image"));

        let resource = CallToolResultContent::EmbeddedResource(EmbeddedResource {
            type_: "resource".to_string(),
            resource: json!({"foo": "bar"}),
            annotations: None,
        });
        let serialized2 = serde_json::to_string(&resource).expect("must succeed");
        assert!(serialized2.contains("resource"));
    }

    #[test]
    fn test_list_tools_request_serialization() {
        let req = ListToolsRequestParams {
            cursor: None,
            meta: None,
        };
        let serialized = serde_json::to_string(&req).expect("must succeed");
        assert!(serialized.contains("{}"));

        let deserialized: ListToolsRequestParams =
            serde_json::from_str(&serialized).expect("must succeed");
        assert!(deserialized.cursor.is_none());
    }

    #[test]
    fn test_list_tools_result_serialization() {
        let res = ListToolsResult {
            meta: None,
            tools: vec![],
            next_cursor: None,
        };
        let serialized = serde_json::to_string(&res).expect("must succeed");
        assert!(serialized.contains("tools"));

        let deserialized: ListToolsResult =
            serde_json::from_str(&serialized).expect("must succeed");
        assert!(deserialized.tools.is_empty());
    }
}

/// Resource definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListResourcesRequestParams {
    /// Cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List resources result.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceRequestParams {
    /// URI.
    pub uri: String,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Resource contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResourceContents {
    /// Text resource.
    Text(TextResourceContents),
    /// Blob resource.
    Blob(BlobResourceContents),
}

/// Text resource contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceResult {
    /// Contents.
    pub contents: Vec<ResourceContents>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Prompt definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListPromptsRequestParams {
    /// Cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List prompts result.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptMessage {
    /// Role.
    pub role: String,
    /// Content.
    pub content: CallToolResultContent,
}

/// Get prompt result.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[cfg(test)]
mod additional_tests_more {
    use super::*;

    #[test]
    fn test_resource_serialization() {
        let r = Resource {
            uri: "file:///a".to_string(),
            name: "a".to_string(),
            description: None,
            mime_type: None,
        };
        assert!(serde_json::to_string(&r)
            .expect("must succeed")
            .contains("file:///a"));
    }

    #[test]
    fn test_read_resource_result() {
        let r = ReadResourceResult {
            meta: None,
            contents: vec![ResourceContents::Text(TextResourceContents {
                uri: "file:///a".to_string(),
                mime_type: None,
                text: "text".to_string(),
            })],
        };
        assert!(serde_json::to_string(&r)
            .expect("must succeed")
            .contains("text"));

        let rb = ReadResourceResult {
            meta: None,
            contents: vec![ResourceContents::Blob(BlobResourceContents {
                uri: "file:///a".to_string(),
                mime_type: None,
                blob: "base64".to_string(),
            })],
        };
        assert!(serde_json::to_string(&rb)
            .expect("must succeed")
            .contains("base64"));
    }

    #[test]
    fn test_prompt_serialization() {
        let p = Prompt {
            name: "test".to_string(),
            description: None,
            arguments: None,
        };
        assert!(serde_json::to_string(&p)
            .expect("must succeed")
            .contains("test"));
    }

    #[test]
    fn test_list_prompts_request_serialization() {
        let req = ListPromptsRequestParams {
            cursor: None,
            meta: None,
        };
        assert!(serde_json::to_string(&req)
            .expect("must succeed")
            .contains("{}"));
    }

    #[test]
    fn test_list_resources_request_serialization() {
        let req = ListResourcesRequestParams {
            cursor: None,
            meta: None,
        };
        assert!(serde_json::to_string(&req)
            .expect("must succeed")
            .contains("{}"));
    }

    #[test]
    fn test_get_prompt_request_serialization() {
        let req = GetPromptRequestParams {
            meta: None,
            name: "a".to_string(),
            arguments: None,
        };
        assert!(serde_json::to_string(&req)
            .expect("must succeed")
            .contains("a"));
    }

    #[test]
    fn test_get_prompt_result_serialization() {
        let res = GetPromptResult {
            meta: None,
            description: None,
            messages: vec![],
        };
        assert!(serde_json::to_string(&res)
            .expect("must succeed")
            .contains("messages"));
    }
}

/// Create message request parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMessageResult {
    /// Role.
    pub role: String,
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

#[cfg(test)]
mod additional_tests2 {
    use super::*;

    #[test]
    fn test_create_message_request_serialization() {
        let req = CreateMessageRequestParams {
            meta: None,
            messages: vec![],
            model_preferences: None,
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            stop_sequences: None,
            include_context: None,
            metadata: None,
        };
        assert!(serde_json::to_string(&req)
            .expect("must succeed")
            .contains("messages"));
    }

    #[test]
    fn test_create_message_result_serialization() {
        let res = CreateMessageResult {
            meta: None,
            role: "user".to_string(),
            content: CallToolResultContent::Text(TextContent {
                text: "t".to_string(),
            }),
            model: "model".to_string(),
            stop_reason: None,
        };
        assert!(serde_json::to_string(&res)
            .expect("must succeed")
            .contains("user"));
    }
}

/// Set logging level request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLevelRequestParams {
    /// Level.
    pub level: String,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Subscribe request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeRequestParams {
    /// URI.
    pub uri: String,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Unsubscribe request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsubscribeRequestParams {
    /// URI.
    pub uri: String,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Logging message notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingMessageNotificationParams {
    /// Level.
    pub level: String,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressNotificationParams {
    /// Progress token.
    pub progress_token: Value,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUpdatedNotificationParams {
    /// URI.
    pub uri: String,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Ping request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingRequestParams {}

/// Cancelled notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelledNotificationParams {
    /// Request ID.
    pub request_id: Value,
    /// Reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List roots request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRootsRequestParams {}

/// Root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    /// URI.
    pub uri: String,
    /// Name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// List roots result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRootsResult {
    /// Roots.
    pub roots: Vec<Root>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Complete request.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteRequestArgument {
    /// Name.
    pub name: String,
    /// Value.
    pub value: String,
}

/// Complete result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteResult {
    /// Completion.
    pub completion: CompleteResultCompletion,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Complete result completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingMessage {
    /// Role.
    pub role: String,
    /// Content.
    pub content: CallToolResultContent,
}

/// Empty result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyResult {}

/// Resource template.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListResourceTemplatesRequestParams {
    /// Cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Meta data.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// List resource templates result.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[cfg(test)]
mod additional_tests3 {
    use super::*;

    #[test]
    fn test_set_level_request_serialization() {
        let req = SetLevelRequestParams {
            meta: None,
            level: "info".to_string(),
        };
        assert!(serde_json::to_string(&req)
            .expect("must succeed")
            .contains("info"));
    }

    #[test]
    fn test_subscribe_request_serialization() {
        let req = SubscribeRequestParams {
            meta: None,
            uri: "file:///a".to_string(),
        };
        assert!(serde_json::to_string(&req)
            .expect("must succeed")
            .contains("file:///a"));
    }

    #[test]
    fn test_unsubscribe_request_serialization() {
        let req = UnsubscribeRequestParams {
            meta: None,
            uri: "file:///a".to_string(),
        };
        assert!(serde_json::to_string(&req)
            .expect("must succeed")
            .contains("file:///a"));
    }

    #[test]
    fn test_logging_message_notification_serialization() {
        let notif = LoggingMessageNotificationParams {
            meta: None,
            level: "info".to_string(),
            logger: None,
            data: Value::Null,
        };
        assert!(serde_json::to_string(&notif)
            .expect("must succeed")
            .contains("info"));
    }

    #[test]
    fn test_progress_notification_serialization() {
        let notif = ProgressNotificationParams {
            meta: None,
            progress_token: Value::Null,
            progress: 1.0,
            total: None,
        };
        assert!(serde_json::to_string(&notif)
            .expect("must succeed")
            .contains("progress"));
    }

    #[test]
    fn test_resource_updated_notification_serialization() {
        let notif = ResourceUpdatedNotificationParams {
            meta: None,
            uri: "file:///a".to_string(),
        };
        assert!(serde_json::to_string(&notif)
            .expect("must succeed")
            .contains("file:///a"));
    }

    #[test]
    fn test_ping_request_serialization() {
        let req = PingRequestParams {};
        assert!(serde_json::to_string(&req)
            .expect("must succeed")
            .contains("{}"));
    }

    #[test]
    fn test_cancelled_notification_serialization() {
        let notif = CancelledNotificationParams {
            meta: None,
            request_id: Value::Null,
            reason: None,
        };
        assert!(serde_json::to_string(&notif)
            .expect("must succeed")
            .contains("requestId"));
    }

    #[test]
    fn test_list_roots_request_serialization() {
        let req = ListRootsRequestParams {};
        assert!(serde_json::to_string(&req)
            .expect("must succeed")
            .contains("{}"));
    }

    #[test]
    fn test_list_roots_result_serialization() {
        let res = ListRootsResult {
            meta: None,
            roots: vec![Root {
                uri: "file:///a".to_string(),
                name: None,
            }],
        };
        assert!(serde_json::to_string(&res)
            .expect("must succeed")
            .contains("file:///a"));
    }

    #[test]
    fn test_complete_request_serialization() {
        let req = CompleteRequestParams {
            meta: None,
            ref_: CompleteRequestReference::Prompt {
                name: "test".to_string(),
            },
            argument: CompleteRequestArgument {
                name: "test".to_string(),
                value: "test".to_string(),
            },
        };
        assert!(serde_json::to_string(&req)
            .expect("must succeed")
            .contains("ref/prompt"));
    }

    #[test]
    fn test_complete_result_serialization() {
        let res = CompleteResult {
            meta: None,
            completion: CompleteResultCompletion {
                values: vec![],
                total: None,
                has_more: None,
            },
        };
        assert!(serde_json::to_string(&res)
            .expect("must succeed")
            .contains("completion"));
    }

    #[test]
    fn test_sampling_message_serialization() {
        let msg = SamplingMessage {
            role: "user".to_string(),
            content: CallToolResultContent::Text(TextContent {
                text: "t".to_string(),
            }),
        };
        assert!(serde_json::to_string(&msg)
            .expect("must succeed")
            .contains("user"));
    }

    #[test]
    fn test_empty_result_serialization() {
        let res = EmptyResult {};
        assert!(serde_json::to_string(&res)
            .expect("must succeed")
            .contains("{}"));
    }

    #[test]
    fn test_resource_template_serialization() {
        let rt = ResourceTemplate {
            uri_template: "test".to_string(),
            name: "test".to_string(),
            description: None,
            mime_type: None,
        };
        assert!(serde_json::to_string(&rt)
            .expect("must succeed")
            .contains("test"));
    }

    #[test]
    fn test_list_resource_templates_request_serialization() {
        let req = ListResourceTemplatesRequestParams {
            cursor: None,
            meta: None,
        };
        assert!(serde_json::to_string(&req)
            .expect("must succeed")
            .contains("{}"));
    }

    #[test]
    fn test_list_resource_templates_result_serialization() {
        let res = ListResourceTemplatesResult {
            meta: None,
            resource_templates: vec![],
            next_cursor: None,
        };
        assert!(serde_json::to_string(&res)
            .expect("must succeed")
            .contains("resourceTemplates"));
    }
}

/// Tool list changed notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolListChangedNotificationParams {}

/// Resource list changed notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceListChangedNotificationParams {}

/// Prompt list changed notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptListChangedNotificationParams {}

/// Roots list changed notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootsListChangedNotificationParams {}

/// Annotated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotated {
    /// Annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

/// Annotations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotations {
    /// Audience.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<String>>,
    /// Priority.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<f64>,
}

/// Client notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializedNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: InitializedNotificationParams,
}

/// Initialized notification params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializedNotificationParams {}

/// Cancelled notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelledNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: CancelledNotificationParams,
}

/// Progress notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: ProgressNotificationParams,
}

/// Roots list changed notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootsListChangedNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: RootsListChangedNotificationParams,
}

/// Server notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUpdatedNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: ResourceUpdatedNotificationParams,
}

/// Resource list changed notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceListChangedNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: ResourceListChangedNotificationParams,
}

/// Tool list changed notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolListChangedNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: ToolListChangedNotificationParams,
}

/// Prompt list changed notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptListChangedNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: PromptListChangedNotificationParams,
}

/// Logging message notification wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingMessageNotification {
    /// Method.
    pub method: String,
    /// Params.
    pub params: LoggingMessageNotificationParams,
}

/// Model hint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHint {
    /// Name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Model preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedRequest {
    /// Method.
    pub method: String,
    /// Params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<PaginatedRequestParams>,
}

/// Paginated request params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedRequestParams {
    /// Cursor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Paginated result.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingRequest {
    /// Method.
    pub method: String,
    /// Params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<PingRequestParams>,
}

/// Initialize request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: InitializeRequestParams,
}

/// Complete request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: CompleteRequestParams,
}

/// Set level request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLevelRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: SetLevelRequestParams,
}

/// Get prompt request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPromptRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: GetPromptRequestParams,
}

/// List prompts request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPromptsRequest {
    /// Method.
    pub method: String,
    /// Params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<ListPromptsRequestParams>,
}

/// List resources request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResourcesRequest {
    /// Method.
    pub method: String,
    /// Params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<ListResourcesRequestParams>,
}

/// List resource templates request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResourceTemplatesRequest {
    /// Method.
    pub method: String,
    /// Params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<ListResourceTemplatesRequestParams>,
}

/// Read resource request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: ReadResourceRequestParams,
}

/// Subscribe request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: SubscribeRequestParams,
}

/// Unsubscribe request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsubscribeRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: UnsubscribeRequestParams,
}

/// Call tool request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: CallToolRequestParams,
}

/// List tools request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsRequest {
    /// Method.
    pub method: String,
    /// Params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<ListToolsRequestParams>,
}

/// Create message request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessageRequest {
    /// Method.
    pub method: String,
    /// Params.
    pub params: CreateMessageRequestParams,
}

/// List roots request wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRootsRequest {
    /// Method.
    pub method: String,
    /// Params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<ListRootsRequestParams>,
}

#[cfg(test)]
mod additional_tests_events {
    use super::*;

    #[test]
    fn test_tool_list_changed_notification_serialization() {
        let notif = ToolListChangedNotificationParams {};
        assert!(serde_json::to_string(&notif)
            .expect("must succeed")
            .contains("{}"));
    }

    #[test]
    fn test_annotated_serialization() {
        let ann = Annotated { annotations: None };
        assert!(serde_json::to_string(&ann)
            .expect("must succeed")
            .contains("{}"));

        let ann2 = Annotated {
            annotations: Some(Annotations {
                audience: Some(vec!["a".to_string()]),
                priority: Some(1.0),
            }),
        };
        assert!(serde_json::to_string(&ann2)
            .expect("must succeed")
            .contains("audience"));
    }

    #[test]
    fn test_client_notification_serialization() {
        let n = ClientNotification::Initialized(InitializedNotification {
            method: "a".to_string(),
            params: InitializedNotificationParams {},
        });
        assert!(serde_json::to_string(&n)
            .expect("must succeed")
            .contains("method"));
    }

    #[test]
    fn test_server_notification_serialization() {
        let n = ServerNotification::ToolListChanged(ToolListChangedNotification {
            method: "a".to_string(),
            params: ToolListChangedNotificationParams {},
        });
        assert!(serde_json::to_string(&n)
            .expect("must succeed")
            .contains("method"));
    }

    #[test]
    fn test_model_preferences_serialization() {
        let p = ModelPreferences {
            hints: Some(vec![ModelHint {
                name: Some("a".to_string()),
            }]),
            cost_priority: None,
            speed_priority: None,
            intelligence_priority: None,
        };
        assert!(serde_json::to_string(&p)
            .expect("must succeed")
            .contains("hints"));
    }

    #[test]
    fn test_client_request_serialization() {
        let req = ClientRequest::Ping(PingRequest {
            method: "a".to_string(),
            params: None,
        });
        assert!(serde_json::to_string(&req)
            .expect("must succeed")
            .contains("method"));
    }

    #[test]
    fn test_server_request_serialization() {
        let req = ServerRequest::Ping(PingRequest {
            method: "a".to_string(),
            params: None,
        });
        assert!(serde_json::to_string(&req)
            .expect("must succeed")
            .contains("method"));
    }

    #[test]
    fn test_client_result_serialization() {
        let res = ClientResult::Empty(EmptyResult {});
        assert!(serde_json::to_string(&res)
            .expect("must succeed")
            .contains("{}"));
    }

    #[test]
    fn test_server_result_serialization() {
        let res = ServerResult::Empty(EmptyResult {});
        assert!(serde_json::to_string(&res)
            .expect("must succeed")
            .contains("{}"));
    }

    #[test]
    fn test_paginated_request_serialization() {
        let req = PaginatedRequest {
            method: "a".to_string(),
            params: None,
        };
        assert!(serde_json::to_string(&req)
            .expect("must succeed")
            .contains("method"));
    }

    #[test]
    fn test_paginated_result_serialization() {
        let res = PaginatedResult {
            next_cursor: None,
            meta: None,
        };
        assert!(serde_json::to_string(&res)
            .expect("must succeed")
            .contains("{}"));
    }
}

/// Logging level.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoggingLevel {
    /// Debug.
    Debug,
    /// Info.
    Info,
    /// Notice.
    Notice,
    /// Warning.
    Warning,
    /// Error.
    Error,
    /// Critical.
    Critical,
    /// Alert.
    Alert,
    /// Emergency.
    Emergency,
}

#[cfg(test)]
mod additional_tests5 {
    use super::*;

    #[test]
    fn test_logging_level_serialization() {
        let level = LoggingLevel::Info;
        assert!(serde_json::to_string(&level)
            .expect("must succeed")
            .contains("info"));
    }
}

/// Cursor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cursor {
    /// Next cursor.
    pub next_cursor: String,
}

/// Request ID.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum RequestId {
    /// String ID.
    String(String),
    /// Number ID.
    Number(i64),
}

/// Progress token.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ProgressToken {
    /// String token.
    String(String),
    /// Number token.
    Number(i64),
}

/// JSON-RPC Message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    /// Request.
    Request(JsonRpcRequest),
    /// Response.
    Response(JsonRpcResponse),
    /// Notification.
    Notification(JsonRpcNotification),
}

/// Role.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// User.
    User,
    /// Assistant.
    Assistant,
}

/// Prompt reference type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptReferenceType {
    /// Prompt.
    Prompt,
}

/// Prompt reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptReference {
    /// Type.
    #[serde(rename = "type")]
    pub ref_type: PromptReferenceType,
    /// Name.
    pub name: String,
}

/// Resource reference type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResourceReferenceType {
    /// Resource.
    Resource,
}

/// Resource reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceReference {
    /// Type.
    #[serde(rename = "type")]
    pub ref_type: ResourceReferenceType,
    /// URI.
    pub uri: String,
}

#[cfg(test)]
mod additional_tests6 {
    use super::*;

    #[test]
    fn test_cursor_serialization() {
        let cursor = Cursor {
            next_cursor: "a".to_string(),
        };
        assert!(serde_json::to_string(&cursor)
            .expect("must succeed")
            .contains("a"));
    }

    #[test]
    fn test_request_id_serialization() {
        let id_str = RequestId::String("a".to_string());
        assert!(serde_json::to_string(&id_str)
            .expect("must succeed")
            .contains("a"));
        let id_num = RequestId::Number(1);
        assert!(serde_json::to_string(&id_num)
            .expect("must succeed")
            .contains("1"));
    }

    #[test]
    fn test_progress_token_serialization() {
        let t_str = ProgressToken::String("a".to_string());
        assert!(serde_json::to_string(&t_str)
            .expect("must succeed")
            .contains("a"));
        let t_num = ProgressToken::Number(1);
        assert!(serde_json::to_string(&t_num)
            .expect("must succeed")
            .contains("1"));
    }

    #[test]
    fn test_json_rpc_message_serialization() {
        let msg = JsonRpcMessage::Request(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::Null,
            method: "m".to_string(),
            params: None,
        });
        assert!(serde_json::to_string(&msg)
            .expect("must succeed")
            .contains("method"));
    }

    #[test]
    fn test_role_serialization() {
        let role = Role::User;
        assert!(serde_json::to_string(&role)
            .expect("must succeed")
            .contains("user"));
    }

    #[test]
    fn test_prompt_reference_serialization() {
        let r = PromptReference {
            ref_type: PromptReferenceType::Prompt,
            name: "a".to_string(),
        };
        assert!(serde_json::to_string(&r)
            .expect("must succeed")
            .contains("prompt"));
    }

    #[test]
    fn test_resource_reference_serialization() {
        let r = ResourceReference {
            ref_type: ResourceReferenceType::Resource,
            uri: "file:///a".to_string(),
        };
        assert!(serde_json::to_string(&r)
            .expect("must succeed")
            .contains("resource"));
    }
}
