#![deny(missing_docs)]

//! # Route Shims
//!
//! Generic structures acting as an Intermediate Deserialization Layer.
//! These structs map directly to OpenAPI YAML objects.
//!
//! Note: Top-level shims do not derive `Debug` because `utoipa::RefOr` and `utoipa::Responses`
//! do not implement `Debug`.

use crate::openapi::parse::resolver::ShimParameter;
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
            } else {
                let path_item = serde_json::from_value::<ShimPathItem>(value).map_err(|e| {
                    DeError::custom(format!("Failed to parse path item '{}': {}", key, e))
                })?;
                items.insert(key, path_item);
            }
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
            } else {
                let item = serde_json::from_value::<RefOr<ShimPathItem>>(value).map_err(|e| {
                    DeError::custom(format!("Failed to parse webhook '{}': {}", key, e))
                })?;
                items.insert(key, item);
            }
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
#[derive(Deserialize, Serialize, Clone, Default)]
pub struct ShimComponents {
    /// Security Schemes.
    #[serde(rename = "securitySchemes")]
    pub security_schemes: Option<BTreeMap<String, RefOr<ShimSecurityScheme>>>,
    /// Reusable Path Item Objects.
    #[serde(rename = "pathItems")]
    pub path_items: Option<BTreeMap<String, RefOr<ShimPathItem>>>,

    /// Document `$self` base URI propagated from the owning OpenAPI root.
    #[serde(skip)]
    pub self_uri: Option<String>,

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
        let inner = serde_json::from_value::<Responses>(sanitized.clone())?;
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
        let inner = serde_json::from_value::<Responses>(sanitized.clone())
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
    if let Some(item_schema_val) = map.get("itemSchema") {
        if item_schema_val.is_boolean() {
            map.remove("itemSchema");
        }
    }
    if let Some(Value::Object(content_map)) = map.get_mut("content") {
        for (_, media) in content_map.iter_mut() {
            if let Value::Object(media_obj) = media {
                media_obj.remove("$ref");
            }
        }
    }

    if let Some(encoding_val) = map.get_mut("encoding") {
        normalize_encoding_map_headers(encoding_val);
    }
    if map.get("prefixEncoding").is_some_and(|v| !v.is_array()) {
        map.remove("prefixEncoding");
    } else if let Some(prefix_val) = map.get_mut("prefixEncoding") {
        normalize_encoding_array_headers(prefix_val);
    }
    if map.get("itemEncoding").is_some_and(|v| !v.is_object()) {
        map.remove("itemEncoding");
    } else if let Some(item_val) = map.get_mut("itemEncoding") {
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
        if obj.get("schema").is_none() {
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
        if let Some(item_schema_val) = obj.get("itemSchema") {
            if item_schema_val.is_boolean() {
                obj.remove("itemSchema");
            }
        }
        if let Some(encoding_val) = obj.get_mut("encoding") {
            normalize_encoding_map_headers(encoding_val);
        }
        if let Some(prefix_val) = obj.get_mut("prefixEncoding") {
            normalize_encoding_array_headers(prefix_val);
        }
        if let Some(item_val) = obj.get_mut("itemEncoding") {
            normalize_encoding_object_headers(item_val);
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
    let headers_val = obj
        .entry("headers".to_string())
        .or_insert(Value::Object(Map::new()));
    normalize_response_headers(headers_val);
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
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
        let path_item: ShimPathItem =
            serde_yaml::from_str(yaml).expect("Failed to parse from string");
        assert!(path_item.query.is_some());
        assert_eq!(
            path_item
                .query
                .expect("Failed to query")
                .operation_id
                .as_deref(),
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
        let op: ShimOperation = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        assert!(op.deprecated);
        assert_eq!(op.summary.as_deref(), Some("Summary line"));
        assert_eq!(op.description.as_deref(), Some("Detailed description"));
        assert!(op.external_docs.is_some());
        assert_eq!(
            op.external_docs.expect("Missing external docs").url,
            "https://example.com"
        );
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let servers = openapi.servers.expect("Missing servers");
        assert_eq!(servers[0].name.as_deref(), Some("prod"));

        let tags = openapi.tags.expect("Missing tags");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let comps = openapi.components.expect("Missing components");
        let schemes = comps.security_schemes.expect("Missing security schemes");
        let serialized_oauth =
            serde_json::to_value(schemes.get("oauth").expect("oauth")).expect("json");
        assert_eq!(
            serialized_oauth["oauth2MetadataUrl"],
            serde_json::json!("https://auth.example.com/.well-known/oauth-authorization-server")
        );
        assert_eq!(
            serialized_oauth["flows"]["deviceAuthorization"]["deviceAuthorizationUrl"],
            serde_json::json!("https://auth.example.com/device")
        );
        assert_eq!(
            serialized_oauth["flows"]["deviceAuthorization"]["tokenUrl"],
            serde_json::json!("https://auth.example.com/token")
        );
        assert!(serialized_oauth["flows"]["deviceAuthorization"]["scopes"]["read"].is_string());
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let comps = openapi.components.expect("Missing components");
        let schemes = comps.security_schemes.expect("Missing security schemes");
        let serialized_key =
            serde_json::to_value(schemes.get("legacyKey").expect("legacyKey")).expect("json");
        assert_eq!(serialized_key["deprecated"], serde_json::json!(true));
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let comps = openapi.components.expect("Missing components");
        let schemes = comps.security_schemes.expect("Missing security schemes");

        let ser_api_key =
            serde_json::to_value(schemes.get("api_key").expect("api_key")).expect("json");
        assert_eq!(ser_api_key["name"], serde_json::json!("api_key"));
        assert_eq!(ser_api_key["in"], serde_json::json!("header"));
        let ser_oauth = serde_json::to_value(schemes.get("oauth").expect("oauth")).expect("json");
        assert_eq!(
            ser_oauth["flows"]["implicit"]["authorizationUrl"],
            serde_json::json!("https://auth.com")
        );
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let definitions = openapi
            .security_definitions
            .expect("Missing security definitions");

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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");

        // Root extension
        let global = openapi
            .extensions
            .get("x-global-config")
            .expect("Failed to get");
        assert_eq!(global["env"], "dev");

        // Info extension
        let info = openapi.info.expect("Missing info");
        assert_eq!(
            info.extensions.get("x-internal").expect("Failed to get"),
            &Value::Bool(true)
        );

        // Operation extension
        let path_item = openapi
            .paths
            .as_ref()
            .and_then(|paths| paths.items.get("/foo"))
            .expect("Failed to and then");
        let op = path_item.get.as_ref().expect("Failed to get reference");
        assert_eq!(
            op.extensions.get("x-controller").expect("Failed to get"),
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");

        let paths = openapi.paths.as_ref().expect("Failed to get reference");
        assert_eq!(
            paths.extensions.get("x-paths-meta"),
            Some(&Value::Bool(true))
        );
        assert!(paths.items.contains_key("/health"));

        let webhooks = openapi.webhooks.as_ref().expect("Failed to get reference");
        assert!(webhooks.extensions.contains_key("x-webhooks-meta"));
        assert!(webhooks.items.contains_key("onEvent"));

        // Cover ShimPaths::is_empty, Serialize for ShimPaths, ShimWebhooks, ShimResponses, ShimRequestBody,
        // and all normalization branches (headers, content, encoding, prefixEncoding, itemEncoding)
        assert!(!paths.is_empty());
        assert!(ShimPaths::default().is_empty());
        assert!(ShimResponses::default().raw().is_object());

        let serialized_openapi = serde_json::to_value(&openapi).expect("serialize openapi");
        assert!(serialized_openapi["paths"]["/health"].is_object());
        assert!(serialized_openapi["webhooks"]["onEvent"].is_object());

        assert!(serde_json::from_value::<ShimPaths>(serde_json::json!({
            "/bad": "not_an_object"
        }))
        .is_err());
        assert!(serde_json::from_value::<ShimWebhooks>(serde_json::json!({
            "badHook": "not_an_object"
        }))
        .is_err());

        let rich_resp_json = serde_json::json!({
            "200": {
                "description": "OK",
                "headers": {
                    "X-Bool": { "schema": true },
                    "X-Ref": { "$ref": "#/components/headers/H" },
                    "X-NoSchema": { "description": "no schema", "content": {} }
                },
                "content": {
                    "application/json": {
                        "$ref": "#/components/mediaTypes/M",
                        "schema": true,
                        "encoding": {
                            "partA": {
                                "headers": {
                                    "X-Enc": { "description": "enc" }
                                }
                            }
                        },
                        "prefixEncoding": [
                            { "contentType": "text/plain" }
                        ],
                        "itemEncoding": {
                            "contentType": "application/octet-stream"
                        }
                    },
                    "text/plain": "not_an_object"
                },
                "links": {
                    "Self": { "operationId": "getSelf" }
                }
            }
        });
        let shim_resp = ShimResponses::from_raw(rich_resp_json.clone()).expect("from_raw");
        assert!(!shim_resp.typed().responses.is_empty());
        assert_eq!(shim_resp.raw(), &rich_resp_json);
        let deser_resp: ShimResponses = serde_json::from_value(rich_resp_json).expect("deser resp");
        let ser_resp = serde_json::to_value(&deser_resp).expect("ser resp");
        assert!(ser_resp.is_object());
        assert!(
            serde_json::from_value::<ShimResponses>(serde_json::json!({"200": "bad"})).is_err()
        );
        assert!(serde_json::from_value::<ShimRequestBody>(serde_json::json!("bad")).is_err());

        let mut non_obj_hdrs = serde_json::json!({"X-Bad": "not_obj"});
        normalize_response_headers(&mut non_obj_hdrs);
        let mut non_obj_resp = serde_json::json!("not_an_object");
        normalize_responses_for_utoipa(&mut non_obj_resp);
        normalize_response_object(&mut non_obj_resp);
        normalize_response_headers(&mut non_obj_resp);
        normalize_response_content(&mut non_obj_resp);
        normalize_encoding_map_headers(&mut non_obj_resp);
        normalize_encoding_array_headers(&mut non_obj_resp);
        normalize_encoding_object_headers(&mut non_obj_resp);
        let mut edge_enc_hdrs = serde_json::json!({
            "content": {
                "application/json": "not_an_object"
            },
            "itemEncoding": "not_an_object"
        });
        normalize_encoding_headers(&mut edge_enc_hdrs);

        let rich_rb_json = serde_json::json!({
            "description": "RB",
            "content": {
                "multipart/mixed": {
                    "schema": true,
                    "encoding": {
                        "partA": {
                            "headers": {
                                "X-Hdr": { "description": "hdr" }
                            }
                        }
                    },
                    "prefixEncoding": [
                        { "contentType": "text/plain" }
                    ],
                    "itemEncoding": {
                        "contentType": "application/octet-stream"
                    }
                }
            }
        });
        let deser_rb: ShimRequestBody =
            serde_json::from_value(rich_rb_json.clone()).expect("deser rb");
        assert_eq!(deser_rb.raw(), &rich_rb_json);
        assert_eq!(deser_rb.typed().description.as_deref(), Some("RB"));
        let ser_rb = serde_json::to_value(&deser_rb).expect("ser rb");
        assert_eq!(ser_rb, rich_rb_json);
        let from_rb = ShimRequestBody::from(deser_rb.inner);
        assert!(from_rb.raw().is_object());
    }
}
