#![deny(missing_docs)]

//! # Route Shims
//!
//! Generic structures acting as an Intermediate Deserialization Layer.
//! These structs map directly to OpenAPI YAML objects.
//!
//! Note: Top-level shims do not derive `Debug` because `utoipa::RefOr` and `utoipa::Responses`
//! do not implement `Debug`.

use crate::oas::resolver::ShimParameter;
use serde::de::Error as DeError;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use utoipa::openapi::request_body::RequestBody;
use utoipa::openapi::{RefOr, Responses};

/// Represents the Paths Object with support for specification extensions.
#[derive(Clone, Default)]
pub struct ShimPaths {
    /// Parsed path items keyed by path template.
    pub items: BTreeMap<String, ShimPathItem>,
    /// Spec extensions attached to the Paths Object (x-...).
    pub extensions: BTreeMap<String, Value>,
}

impl ShimPaths {
    /// Returns true when no concrete path items are present.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<'de> Deserialize<'de> for ShimPaths {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = BTreeMap::<String, Value>::deserialize(deserializer)?;
        let mut items = BTreeMap::new();
        let mut extensions = BTreeMap::new();

        for (key, value) in raw {
            if key.starts_with("x-") {
                extensions.insert(key, value);
                continue;
            }
            let path_item = serde_json::from_value::<ShimPathItem>(value).map_err(|e| {
                DeError::custom(format!("Failed to parse path item '{}': {}", key, e))
            })?;
            items.insert(key, path_item);
        }

        Ok(Self { items, extensions })
    }
}

impl Serialize for ShimPaths {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.items.len() + self.extensions.len()))?;
        for (key, value) in &self.items {
            map.serialize_entry(key, value)?;
        }
        for (key, value) in &self.extensions {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

/// Represents the Webhooks Object with support for specification extensions.
#[derive(Clone, Default)]
pub struct ShimWebhooks {
    /// Parsed webhook path items keyed by name.
    pub items: BTreeMap<String, RefOr<ShimPathItem>>,
    /// Spec extensions attached to the Webhooks Object (x-...).
    pub extensions: BTreeMap<String, Value>,
}

impl<'de> Deserialize<'de> for ShimWebhooks {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = BTreeMap::<String, Value>::deserialize(deserializer)?;
        let mut items = BTreeMap::new();
        let mut extensions = BTreeMap::new();

        for (key, value) in raw {
            if key.starts_with("x-") {
                extensions.insert(key, value);
                continue;
            }
            let item = serde_json::from_value::<RefOr<ShimPathItem>>(value).map_err(|e| {
                DeError::custom(format!("Failed to parse webhook '{}': {}", key, e))
            })?;
            items.insert(key, item);
        }

        Ok(Self { items, extensions })
    }
}

impl Serialize for ShimWebhooks {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.items.len() + self.extensions.len()))?;
        for (key, value) in &self.items {
            map.serialize_entry(key, value)?;
        }
        for (key, value) in &self.extensions {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

/// Schema for the root document (Paths and Webhooks).
#[derive(Deserialize, Serialize)]
pub struct ShimOpenApi {
    /// OpenAPI version (e.g. "3.2.0").
    /// Required in OAS 3.x.
    pub openapi: Option<String>,

    /// Swagger version (e.g. "2.0") for legacy support.
    pub swagger: Option<String>,

    /// The `$self` keyword (OAS 3.2+).
    /// Establishes the Base URI for the document.
    #[serde(rename = "$self")]
    pub self_uri: Option<String>,

    /// Metadata about the API.
    /// Required in OAS 3.x.
    pub info: Option<ShimInfo>,

    /// JSON Schema dialect (OAS 3.1+).
    /// Default value for the $schema keyword within Schema Objects.
    #[serde(rename = "jsonSchemaDialect")]
    pub json_schema_dialect: Option<String>,

    /// Components section used for reference resolution (OAS 3.x).
    #[serde(default)]
    pub components: Option<ShimComponents>,

    /// Security Definitions (OAS 2.0 Legacy).
    #[serde(rename = "securityDefinitions", default)]
    pub security_definitions: Option<BTreeMap<String, ShimSecurityScheme>>,

    /// Server configuration (OAS 3.x).
    #[serde(default)]
    pub servers: Option<Vec<ShimServer>>,

    /// Base path (Swagger 2.0 Legacy).
    /// Prepended to all paths in valid Swagger 2.0 docs.
    #[serde(rename = "basePath")]
    pub base_path: Option<String>,

    /// Path items.
    pub paths: Option<ShimPaths>,

    /// Webhook items.
    pub webhooks: Option<ShimWebhooks>,

    /// Global security requirements.
    #[serde(default)]
    pub security: Option<Vec<Value>>,

    /// Tags used by the specification with additional metadata.
    #[serde(default)]
    pub tags: Option<Vec<ShimTag>>,

    /// External documentation.
    #[serde(rename = "externalDocs")]
    pub external_docs: Option<ShimExternalDocs>,

    /// Specification Extensions (x-...).
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Components object (OAS 3.x) holding reusable definitions.
///
/// We do not derive `Debug` here because `RefOr` in utoipa 5.x dependency tree
/// may not implement `Debug` for all internal variations, causing build failures.
#[derive(Deserialize, Serialize, Clone)]
pub struct ShimComponents {
    /// Security Schemes.
    #[serde(rename = "securitySchemes")]
    pub security_schemes: Option<BTreeMap<String, RefOr<ShimSecurityScheme>>>,
    /// Reusable Path Item Objects.
    #[serde(rename = "pathItems")]
    pub path_items: Option<BTreeMap<String, RefOr<ShimPathItem>>>,

    /// Capture other loosely typed component maps (schemas, parameters, etc.)
    /// to maintain compatibility with existing resolvers that expect generic Value lookups.
    ///
    /// Note: This field effectively captures both standard component types we don't strictly type yet
    /// AND specification extensions at the component level.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Strict definition of Security Schemes (Basic, API Key, OAuth2, etc.).
/// Supports both OAS 2.0 and 3.x fields.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "type")]
pub enum ShimSecurityScheme {
    /// SSH/API Key.
    #[serde(rename = "apiKey")]
    ApiKey(ShimApiKey),
    /// HTTP Authentication (Basic, Bearer).
    #[serde(rename = "http")]
    Http(ShimHttpAuth),
    /// OAuth2.
    #[serde(rename = "oauth2")]
    OAuth2(Box<ShimOAuth2>),
    /// OpenID Connect.
    #[serde(rename = "openIdConnect")]
    OpenIdConnect(ShimOpenIdConnect),
    /// Mutual TLS.
    #[serde(rename = "mutualTLS")]
    MutualTls(ShimMutualTls),
    /// Basic Auth (Legacy OAS 2.0 implicit type).
    #[serde(rename = "basic")]
    Basic,
}

/// API Key definition.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ShimApiKey {
    /// Parameter name (header/query/cookie name).
    pub name: String,
    /// Location (query, header, cookie).
    #[serde(rename = "in")]
    pub in_loc: String,
    /// Description.
    pub description: Option<String>,
    /// Whether this security scheme is deprecated.
    #[serde(default)]
    pub deprecated: bool,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// HTTP Authentication definition.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ShimHttpAuth {
    /// Scheme (basic, bearer, etc.).
    pub scheme: String,
    /// Format (e.g. JWT).
    #[serde(rename = "bearerFormat")]
    pub bearer_format: Option<String>,
    /// Description.
    pub description: Option<String>,
    /// Whether this security scheme is deprecated.
    #[serde(default)]
    pub deprecated: bool,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// OpenID Connect definition.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ShimOpenIdConnect {
    /// Connect URL.
    #[serde(rename = "openIdConnectUrl")]
    pub open_id_connect_url: String,
    /// Description.
    pub description: Option<String>,
    /// Whether this security scheme is deprecated.
    #[serde(default)]
    pub deprecated: bool,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Mutual TLS definition.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ShimMutualTls {
    /// Description.
    pub description: Option<String>,
    /// Whether this security scheme is deprecated.
    #[serde(default)]
    pub deprecated: bool,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// OAuth2 definition.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ShimOAuth2 {
    /// Description.
    pub description: Option<String>,
    /// Whether this security scheme is deprecated.
    #[serde(default)]
    pub deprecated: bool,
    /// Supported flows.
    pub flows: Option<ShimOAuthFlows>,
    /// URL to OAuth2 authorization server metadata (RFC8414).
    #[serde(rename = "oauth2MetadataUrl")]
    pub oauth2_metadata_url: Option<String>,
    // OAS 2.0 Compatibility Fields (flattened here conceptually, handled via specific fields if needed)
    /// Flow (OAS 2.0 specific: implicit, password, application, accessCode).
    /// If present (legacy), it maps loosely to one of the flows below.
    #[serde(rename = "flow")]
    pub flow: Option<String>,
    /// Authorization URL (OAS 2.0 legacy).
    #[serde(rename = "authorizationUrl")]
    pub authorization_url: Option<String>,
    /// Token URL (OAS 2.0 legacy).
    #[serde(rename = "tokenUrl")]
    pub token_url: Option<String>,
    /// Scopes (OAS 2.0 legacy).
    pub scopes: Option<BTreeMap<String, String>>,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Container for OAuth2 Flows (OAS 3.x).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ShimOAuthFlows {
    /// Implicit Flow.
    pub implicit: Option<ShimOAuthFlow>,
    /// Resource Owner Password Flow.
    pub password: Option<ShimOAuthFlow>,
    /// Client Credentials Flow.
    #[serde(rename = "clientCredentials")]
    pub client_credentials: Option<ShimOAuthFlow>,
    /// Authorization Code Flow.
    #[serde(rename = "authorizationCode")]
    pub authorization_code: Option<ShimOAuthFlow>,
    /// Device Authorization Flow.
    #[serde(rename = "deviceAuthorization")]
    pub device_authorization: Option<ShimOAuthFlow>,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Single OAuth Flow definition.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ShimOAuthFlow {
    /// Authorization URL.
    #[serde(rename = "authorizationUrl")]
    pub authorization_url: Option<String>,
    /// Device Authorization URL (RFC8628).
    #[serde(rename = "deviceAuthorizationUrl")]
    pub device_authorization_url: Option<String>,
    /// Token URL.
    #[serde(rename = "tokenUrl")]
    pub token_url: Option<String>,
    /// Refresh URL.
    #[serde(rename = "refreshUrl")]
    pub refresh_url: Option<String>,
    /// Available scopes and descriptions.
    #[serde(default)]
    pub scopes: BTreeMap<String, String>,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Metadata about the API (Info Object).
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ShimInfo {
    /// The title of the API.
    pub title: String,
    /// A short summary of the API.
    pub summary: Option<String>,
    /// A description of the API.
    pub description: Option<String>,
    /// A URL to the Terms of Service for the API.
    #[serde(rename = "termsOfService")]
    pub terms_of_service: Option<String>,
    /// The contact information for the exposed API.
    pub contact: Option<ShimContact>,
    /// The license information for the exposed API.
    pub license: Option<ShimLicense>,
    /// The version of the OpenAPI document.
    pub version: String,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Contact information for the exposed API.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ShimContact {
    /// The identifying name of the contact person/organization.
    pub name: Option<String>,
    /// The URL pointing to the contact information.
    pub url: Option<String>,
    /// The email address of the contact person/organization.
    pub email: Option<String>,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// License information for the exposed API.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ShimLicense {
    /// The license name used for the API.
    pub name: String,
    /// An SPDX license expression for the API (OAS 3.1+).
    pub identifier: Option<String>,
    /// A URL to the license used for the API.
    pub url: Option<String>,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// An object representing a Server.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ShimServer {
    /// A URL to the target host.
    pub url: String,
    /// An optional string describing the host.
    pub description: Option<String>,
    /// An optional unique string to refer to the host.
    pub name: Option<String>,
    /// A map between a variable name and its value.
    #[serde(default)]
    pub variables: Option<BTreeMap<String, ShimServerVariable>>,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// An object representing a Server Variable.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ShimServerVariable {
    /// An enumeration of string values to be used if the substitution options are from a limited set.
    #[serde(rename = "enum")]
    pub enum_values: Option<Vec<String>>,
    /// The default value to use for substitution.
    pub default: String,
    /// An optional description for the server variable.
    pub description: Option<String>,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Allows referencing an external resource for extended documentation.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ShimExternalDocs {
    /// A description of the target documentation.
    pub description: Option<String>,
    /// The URL for the target documentation.
    pub url: String,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Adds metadata to a single tag that is used by the Operation Object.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ShimTag {
    /// The name of the tag.
    pub name: String,
    /// A short summary of the tag.
    pub summary: Option<String>,
    /// A description for the tag.
    pub description: Option<String>,
    /// Additional external documentation for this tag.
    #[serde(rename = "externalDocs")]
    pub external_docs: Option<ShimExternalDocs>,
    /// The parent tag name for nesting.
    pub parent: Option<String>,
    /// The tag kind (e.g., nav, badge, audience).
    pub kind: Option<String>,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// A Path Item containing operations for a specific URL/Webhook.
#[derive(Deserialize, Serialize, Clone)]
pub struct ShimPathItem {
    /// Allows for a referenced definition of this path item.
    #[serde(rename = "$ref")]
    pub ref_path: Option<String>,
    /// Optional summary for all operations in this path.
    pub summary: Option<String>,
    /// Optional description for all operations in this path.
    pub description: Option<String>,
    /// Alternative server array for this path item.
    pub servers: Option<Vec<ShimServer>>,
    /// Parameters common to all operations in this path.
    #[serde(default)]
    pub parameters: Option<Vec<RefOr<ShimParameter>>>,
    /// GET operation.
    pub get: Option<ShimOperation>,
    /// POST operation.
    pub post: Option<ShimOperation>,
    /// PUT operation.
    pub put: Option<ShimOperation>,
    /// DELETE operation.
    pub delete: Option<ShimOperation>,
    /// PATCH operation.
    pub patch: Option<ShimOperation>,
    /// OPTIONS operation.
    pub options: Option<ShimOperation>,
    /// HEAD operation.
    pub head: Option<ShimOperation>,
    /// TRACE operation.
    pub trace: Option<ShimOperation>,
    /// QUERY operation (OAS 3.2+).
    pub query: Option<ShimOperation>,
    /// Map of additional operations keyed by custom HTTP methods.
    #[serde(rename = "additionalOperations")]
    pub additional_operations: Option<BTreeMap<String, ShimOperation>>,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

/// Wrapper for Responses that preserves raw JSON to access OAS 3.2-only fields.
///
/// This enables extraction of `itemSchema` and other Media Type fields that are
/// not modeled by `utoipa::openapi::Responses`.
#[derive(Clone)]
pub struct ShimResponses {
    /// Raw JSON representation of the responses map.
    pub raw: Value,
    /// Parsed Responses model from utoipa.
    pub inner: Responses,
}

impl ShimResponses {
    /// Returns the parsed Responses model.
    pub fn typed(&self) -> &Responses {
        &self.inner
    }

    /// Returns the raw JSON representation.
    pub fn raw(&self) -> &Value {
        &self.raw
    }

    /// Builds a ShimResponses wrapper from raw JSON.
    pub fn from_raw(raw: Value) -> Result<Self, serde_json::Error> {
        let mut sanitized = raw.clone();
        normalize_responses_for_utoipa(&mut sanitized);
        let inner = serde_json::from_value::<Responses>(sanitized)?;
        Ok(Self { raw, inner })
    }
}

impl Default for ShimResponses {
    fn default() -> Self {
        Self {
            raw: Value::Object(Map::new()),
            inner: Responses::new(),
        }
    }
}

impl<'de> Deserialize<'de> for ShimResponses {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = Value::deserialize(deserializer)?;
        let mut sanitized = raw.clone();
        normalize_responses_for_utoipa(&mut sanitized);
        let inner = serde_json::from_value::<Responses>(sanitized)
            .map_err(|e| DeError::custom(format!("Failed to parse Responses: {}", e)))?;
        Ok(Self { raw, inner })
    }
}

impl Serialize for ShimResponses {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.raw.serialize(serializer)
    }
}

impl From<Responses> for ShimResponses {
    fn from(responses: Responses) -> Self {
        let raw = serde_json::to_value(&responses).unwrap_or(Value::Object(Map::new()));
        Self {
            raw,
            inner: responses,
        }
    }
}

/// Wrapper for RequestBody that preserves raw JSON to access OAS 3.2-only fields.
///
/// This enables extraction of `itemSchema`, `serializedValue`, and other fields
/// that are not yet modeled by `utoipa::openapi::RequestBody`.
#[derive(Clone)]
pub struct ShimRequestBody {
    /// Raw JSON representation of the request body.
    pub raw: Value,
    /// Parsed RequestBody model from utoipa.
    pub inner: RequestBody,
}

impl ShimRequestBody {
    /// Returns the parsed RequestBody model.
    pub fn typed(&self) -> &RequestBody {
        &self.inner
    }

    /// Returns the raw JSON representation.
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}

impl<'de> Deserialize<'de> for ShimRequestBody {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = Value::deserialize(deserializer)?;
        let mut sanitized = raw.clone();
        normalize_encoding_headers(&mut sanitized);
        let inner = serde_json::from_value::<RequestBody>(sanitized)
            .map_err(|e| DeError::custom(format!("Failed to parse RequestBody: {}", e)))?;
        Ok(Self { raw, inner })
    }
}

impl Serialize for ShimRequestBody {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.raw.serialize(serializer)
    }
}

impl From<RequestBody> for ShimRequestBody {
    fn from(body: RequestBody) -> Self {
        let raw = serde_json::to_value(&body).unwrap_or(Value::Null);
        Self { raw, inner: body }
    }
}

fn normalize_encoding_headers(value: &mut Value) {
    let Value::Object(map) = value else {
        return;
    };

    if let Some(schema_val) = map.get("schema") {
        if schema_val.is_boolean() {
            map.remove("schema");
        }
    }

    if let Some(encoding_val) = map.get_mut("encoding") {
        normalize_encoding_map_headers(encoding_val);
    }
    if let Some(prefix_val) = map.get_mut("prefixEncoding") {
        normalize_encoding_array_headers(prefix_val);
    }
    if let Some(item_val) = map.get_mut("itemEncoding") {
        normalize_encoding_object_headers(item_val);
    }

    for (_, v) in map.iter_mut() {
        normalize_encoding_headers(v);
    }
}

fn normalize_responses_for_utoipa(value: &mut Value) {
    let Value::Object(map) = value else {
        return;
    };

    for (_, response) in map.iter_mut() {
        normalize_response_object(response);
    }
}

fn normalize_response_object(value: &mut Value) {
    let Value::Object(map) = value else {
        return;
    };

    if let Some(headers_val) = map.get_mut("headers") {
        normalize_response_headers(headers_val);
    }

    if let Some(content_val) = map.get_mut("content") {
        normalize_response_content(content_val);
    }

    if map.contains_key("links") {
        map.remove("links");
    }
}

fn normalize_response_headers(value: &mut Value) {
    let Value::Object(headers) = value else {
        return;
    };

    for (_, header) in headers.iter_mut() {
        let Value::Object(obj) = header else {
            continue;
        };
        if let Some(schema_val) = obj.get("schema") {
            if schema_val.is_boolean() {
                obj.remove("schema");
                let mut schema = Map::new();
                schema.insert("type".to_string(), Value::String("string".to_string()));
                obj.insert("schema".to_string(), Value::Object(schema));
            }
        }
        if obj.contains_key("$ref") {
            obj.clear();
            let mut schema = Map::new();
            schema.insert("type".to_string(), Value::String("string".to_string()));
            obj.insert("schema".to_string(), Value::Object(schema));
            continue;
        }
        if obj.get("schema").is_none() && obj.contains_key("content") {
            let mut schema = Map::new();
            schema.insert("type".to_string(), Value::String("string".to_string()));
            obj.insert("schema".to_string(), Value::Object(schema));
        } else if obj.get("schema").is_none() && !obj.contains_key("content") {
            let mut schema = Map::new();
            schema.insert("type".to_string(), Value::String("string".to_string()));
            obj.insert("schema".to_string(), Value::Object(schema));
        }
        obj.remove("content");
    }
}

fn normalize_response_content(value: &mut Value) {
    let Value::Object(content) = value else {
        return;
    };

    for (_, media) in content.iter_mut() {
        let Value::Object(obj) = media else {
            *media = Value::Object(Map::new());
            continue;
        };
        if obj.contains_key("$ref") {
            obj.remove("$ref");
        }
        if let Some(schema_val) = obj.get("schema") {
            if schema_val.is_boolean() {
                obj.remove("schema");
            }
        }
    }
}

fn normalize_encoding_map_headers(value: &mut Value) {
    let Value::Object(map) = value else {
        return;
    };
    for (_, encoding) in map.iter_mut() {
        normalize_encoding_object_headers(encoding);
    }
}

fn normalize_encoding_array_headers(value: &mut Value) {
    let Value::Array(items) = value else {
        return;
    };
    for encoding in items.iter_mut() {
        normalize_encoding_object_headers(encoding);
    }
}

fn normalize_encoding_object_headers(value: &mut Value) {
    let Value::Object(obj) = value else {
        return;
    };
    obj.entry("headers".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
}

/// A single HTTP Operation definition.
#[derive(Deserialize, Serialize, Clone)]
pub struct ShimOperation {
    /// Unique identifier for the operation.
    #[serde(rename = "operationId")]
    pub operation_id: Option<String>,
    /// A short summary of what the operation does.
    pub summary: Option<String>,
    /// A verbose explanation of the operation behavior.
    pub description: Option<String>,
    /// Operation-specific parameters.
    #[serde(default)]
    pub parameters: Option<Vec<RefOr<ShimParameter>>>,
    /// Request Body.
    #[serde(rename = "requestBody")]
    pub request_body: Option<RefOr<ShimRequestBody>>,
    /// Responses.
    #[serde(default)]
    pub responses: ShimResponses,
    /// Security requirements (raw JSON values to be generic).
    #[serde(default)]
    pub security: Option<Vec<Value>>,
    /// Callbacks definition.
    #[serde(default)]
    pub callbacks: Option<BTreeMap<String, RefOr<BTreeMap<String, ShimPathItem>>>>,
    /// A list of tags for API documentation control.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Whether this operation is deprecated.
    #[serde(default)]
    pub deprecated: bool,
    /// External documentation definition.
    #[serde(rename = "externalDocs")]
    pub external_docs: Option<ShimExternalDocs>,
    /// Alternative server array for this operation.
    #[serde(default)]
    pub servers: Option<Vec<ShimServer>>,
    /// Extensions.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shim_openapi_dialect_parsing() {
        let yaml = r#"
openapi: 3.2.0
jsonSchemaDialect: https://spec.openapis.org/oas/3.1/dialect/base
info:
  title: Dialect Test
  version: 1.0.0
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            openapi.json_schema_dialect.as_deref(),
            Some("https://spec.openapis.org/oas/3.1/dialect/base")
        );
    }

    #[test]
    fn test_shim_self_uri_parsing() {
        let yaml = r#"
openapi: 3.2.0
$self: https://example.com/api/v1/definition.yaml
info:
  title: Self Test
  version: 1.0.0
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            openapi.self_uri.as_deref(),
            Some("https://example.com/api/v1/definition.yaml")
        );
    }

    #[test]
    fn test_shim_path_item_query_method_parsing() {
        // Test ensuring 'query' field is parseable in ShimPathItem
        let yaml = r#"
query:
  operationId: queryOp
  responses:
    '200': { description: OK }
"#;
        let path_item: ShimPathItem = serde_yaml::from_str(yaml).unwrap();
        assert!(path_item.query.is_some());
        assert_eq!(
            path_item.query.unwrap().operation_id.as_deref(),
            Some("queryOp")
        );
    }

    #[test]
    fn test_shim_operation_metadata() {
        let yaml = r#"
operationId: testOp
summary: Summary line
description: Detailed description
deprecated: true
externalDocs:
  url: https://example.com
  description: More info
responses:
  '200': { description: OK }
"#;
        let op: ShimOperation = serde_yaml::from_str(yaml).unwrap();
        assert!(op.deprecated);
        assert_eq!(op.summary.as_deref(), Some("Summary line"));
        assert_eq!(op.description.as_deref(), Some("Detailed description"));
        assert!(op.external_docs.is_some());
        assert_eq!(op.external_docs.unwrap().url, "https://example.com");
    }

    #[test]
    fn test_shim_server_name_and_tag_hierarchy() {
        let yaml = r#"
openapi: 3.2.0
info:
  title: Meta
  version: 1.0
servers:
  - url: https://api.example.com
    name: prod
tags:
  - name: external
    summary: External
    kind: audience
  - name: partner
    parent: external
    kind: audience
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let servers = openapi.servers.unwrap();
        assert_eq!(servers[0].name.as_deref(), Some("prod"));

        let tags = openapi.tags.unwrap();
        assert_eq!(tags[0].name, "external");
        assert_eq!(tags[0].kind.as_deref(), Some("audience"));
        assert_eq!(tags[1].parent.as_deref(), Some("external"));
    }

    #[test]
    fn test_shim_oauth2_device_flow_and_metadata() {
        let yaml = r#"
openapi: 3.2.0
info:
  title: OAuth
  version: 1.0
components:
  securitySchemes:
    oauth:
      type: oauth2
      oauth2MetadataUrl: https://auth.example.com/.well-known/oauth-authorization-server
      flows:
        deviceAuthorization:
          deviceAuthorizationUrl: https://auth.example.com/device
          tokenUrl: https://auth.example.com/token
          scopes:
            read: read stuff
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let comps = openapi.components.unwrap();
        let schemes = comps.security_schemes.unwrap();
        let oauth = match schemes.get("oauth").unwrap() {
            RefOr::T(ShimSecurityScheme::OAuth2(o)) => o,
            _ => panic!("Expected OAuth2"),
        };
        assert_eq!(
            oauth.oauth2_metadata_url.as_deref(),
            Some("https://auth.example.com/.well-known/oauth-authorization-server")
        );
        let flow = oauth
            .flows
            .as_ref()
            .unwrap()
            .device_authorization
            .as_ref()
            .unwrap();
        assert_eq!(
            flow.device_authorization_url.as_deref(),
            Some("https://auth.example.com/device")
        );
        assert_eq!(
            flow.token_url.as_deref(),
            Some("https://auth.example.com/token")
        );
        assert!(flow.scopes.contains_key("read"));
    }

    #[test]
    fn test_shim_security_scheme_deprecated() {
        let yaml = r#"
openapi: 3.2.0
info:
  title: Deprecated Scheme
  version: 1.0
components:
  securitySchemes:
    legacyKey:
      type: apiKey
      name: X-LEGACY
      in: header
      deprecated: true
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let comps = openapi.components.unwrap();
        let schemes = comps.security_schemes.unwrap();
        let legacy = match schemes.get("legacyKey").unwrap() {
            RefOr::T(ShimSecurityScheme::ApiKey(k)) => k,
            _ => panic!("Expected apiKey scheme"),
        };
        assert!(legacy.deprecated);
    }

    #[test]
    fn test_shim_security_schemes_parsing() {
        let yaml = r#"
openapi: 3.0.0
info:
  title: Security Test
  version: 1.0
components:
  securitySchemes:
    api_key:
      type: apiKey
      name: api_key
      in: header
    jwt:
      type: http
      scheme: bearer
      bearerFormat: JWT
    oauth:
      type: oauth2
      flows:
        implicit:
          authorizationUrl: https://auth.com
          scopes:
            read: read stuff
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let comps = openapi.components.unwrap();
        let schemes = comps.security_schemes.unwrap();

        // Check API Key
        let api_key = match schemes.get("api_key").unwrap() {
            RefOr::T(ShimSecurityScheme::ApiKey(k)) => k,
            _ => panic!("Expected ApiKey"),
        };
        assert_eq!(api_key.name, "api_key");
        assert_eq!(api_key.in_loc, "header");

        // Check OAuth2
        let oauth = match schemes.get("oauth").unwrap() {
            RefOr::T(ShimSecurityScheme::OAuth2(o)) => o,
            _ => panic!("Expected OAuth2"),
        };
        let flow = oauth.flows.as_ref().unwrap().implicit.as_ref().unwrap();
        assert_eq!(flow.authorization_url.as_deref(), Some("https://auth.com"));
        assert!(flow.scopes.contains_key("read"));
    }

    #[test]
    fn test_shim_legacy_security_parsing() {
        let yaml = r#"
swagger: "2.0"
info: {title: Legacy, version: 1}
securityDefinitions:
  basicAuth:
    type: basic
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let definitions = openapi.security_definitions.unwrap();

        assert!(matches!(
            definitions.get("basicAuth"),
            Some(ShimSecurityScheme::Basic)
        ));
    }

    #[test]
    fn test_shim_swagger_base_path() {
        let yaml = r#"
swagger: "2.0"
info: {title: Legacy with Base, version: 1}
basePath: /api/v1
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(openapi.base_path.as_deref(), Some("/api/v1"));
    }

    #[test]
    fn test_extensions_captured() {
        // Test that x- vendor extensions are preserved
        let yaml = r#"
openapi: 3.2.0
info:
  title: Ext test
  version: 1.0
  x-internal: true
paths:
  /foo:
    get:
      x-controller: FooController
      responses:
        '200': { description: OK }
x-global-config:
  env: dev
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();

        // Root extension
        let global = openapi.extensions.get("x-global-config").unwrap();
        assert_eq!(global["env"], "dev");

        // Info extension
        let info = openapi.info.unwrap();
        assert_eq!(
            info.extensions.get("x-internal").unwrap(),
            &Value::Bool(true)
        );

        // Operation extension
        let path_item = openapi
            .paths
            .as_ref()
            .and_then(|paths| paths.items.get("/foo"))
            .unwrap();
        let op = path_item.get.as_ref().unwrap();
        assert_eq!(
            op.extensions.get("x-controller").unwrap(),
            &Value::String("FooController".to_string())
        );
    }

    #[test]
    fn test_paths_and_webhooks_extensions_captured() {
        let yaml = r#"
openapi: 3.2.0
info:
  title: Paths Ext
  version: 1.0
paths:
  x-paths-meta: true
  /health:
    get:
      responses:
        '200': { description: OK }
webhooks:
  x-webhooks-meta:
    enabled: true
  onEvent:
    post:
      responses:
        '200': { description: OK }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();

        let paths = openapi.paths.as_ref().unwrap();
        assert_eq!(
            paths.extensions.get("x-paths-meta"),
            Some(&Value::Bool(true))
        );
        assert!(paths.items.contains_key("/health"));

        let webhooks = openapi.webhooks.as_ref().unwrap();
        assert!(webhooks.extensions.contains_key("x-webhooks-meta"));
        assert!(webhooks.items.contains_key("onEvent"));
    }
}
