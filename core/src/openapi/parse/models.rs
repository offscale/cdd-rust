#![deny(missing_docs)]

//! # OpenAPI Models
//!
//! definition of Intermediate Representation (IR) structures for parsed OpenAPI elements.
//!
//! These structs are used to transport parsed data from the YAML spec
//! into the code generation strategies.

use crate::classes::parse::models::ParsedExternalDocs;
use crate::error::{AppError, AppResult};
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, HashMap};
use std::fmt;

/// Distinguishes between standard paths and event-driven webhooks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteKind {
    /// A standard HTTP endpoint defined in `paths`.
    Path,
    /// An event receiver defined in `webhooks`.
    Webhook,
}

/// A server variable definition for an OpenAPI Server Object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedServerVariable {
    /// Optional enumeration of allowed values.
    pub enum_values: Option<Vec<String>>,
    /// Default value used for substitution.
    pub default: String,
    /// Optional description of the server variable.
    pub description: Option<String>,
}

impl ParsedServerVariable {
    /// Creates a new server variable with the required default value.
    ///
    /// # Arguments
    /// * `default` - Default substitution value.
    ///
    /// # Returns
    /// * Initialized [`ParsedServerVariable`].
    pub fn new(default: impl Into<String>) -> Self {
        Self {
            enum_values: None,
            default: default.into(),
            description: None,
        }
    }

    /// Sets enum values for the variable.
    ///
    /// # Arguments
    /// * `values` - Allowed enumeration strings.
    ///
    /// # Returns
    /// * Updated [`ParsedServerVariable`].
    pub fn with_enum_values(mut self, values: Vec<String>) -> Self {
        self.enum_values = Some(values);
        self
    }

    /// Sets the variable description.
    ///
    /// # Arguments
    /// * `description` - Human-readable description.
    ///
    /// # Returns
    /// * Updated [`ParsedServerVariable`].
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// A parsed OpenAPI Server Object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedServer {
    /// A URL to the target host (may include variables).
    pub url: String,
    /// Optional description of the server.
    pub description: Option<String>,
    /// Optional unique name for the server.
    pub name: Option<String>,
    /// Optional map of server variables.
    pub variables: BTreeMap<String, ParsedServerVariable>,
}

impl ParsedServer {
    /// Creates a new server with the required URL.
    ///
    /// # Arguments
    /// * `url` - Target server URL or template.
    ///
    /// # Returns
    /// * Initialized [`ParsedServer`].
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            description: None,
            name: None,
            variables: BTreeMap::new(),
        }
    }

    /// Sets the server description.
    ///
    /// # Arguments
    /// * `description` - Server description text.
    ///
    /// # Returns
    /// * Updated [`ParsedServer`].
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the server name.
    ///
    /// # Arguments
    /// * `name` - Unique server name.
    ///
    /// # Returns
    /// * Updated [`ParsedServer`].
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Adds a server variable definition.
    ///
    /// # Arguments
    /// * `name` - Variable placeholder name.
    /// * `variable` - Variable definition.
    ///
    /// # Returns
    /// * Updated [`ParsedServer`].
    pub fn with_variable(
        mut self,
        name: impl Into<String>,
        variable: ParsedServerVariable,
    ) -> Self {
        self.variables.insert(name.into(), variable);
        self
    }

    /// Replaces the server variables map.
    ///
    /// # Arguments
    /// * `variables` - Map of variable definitions.
    ///
    /// # Returns
    /// * Updated [`ParsedServer`].
    pub fn with_variables(mut self, variables: BTreeMap<String, ParsedServerVariable>) -> Self {
        self.variables = variables;
        self
    }
}

/// Represents a validated Runtime Expression or template (OAS 3.2 ABNF).
///
/// Syntax: `$url` | `$method` | `$statusCode` | `$request.{source}` | `$response.{source}`
/// Templates may embed expressions inside `{}` within larger strings.
#[derive(Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct RuntimeExpression(String);

impl RuntimeExpression {
    /// Internal test constructor that normalizes and validates the expression.
    #[cfg(test)]
    pub(crate) fn new(s: impl Into<String>) -> Self {
        let raw = s.into();
        let normalized = normalize_runtime_expression(&raw);
        Self(normalized)
    }

    /// Parses and validates a runtime expression or template, allowing constants.
    ///
    /// If the value is not a runtime expression (does not start with `$` and
    /// contains no `{...}` runtime segments), the value is accepted as a literal.
    pub fn parse(raw: &str) -> AppResult<Self> {
        let normalized = normalize_runtime_expression(raw);
        if normalized.starts_with('$') {
            validate_runtime_expression(&normalized)?;
        } else {
            for seg in split_runtime_expression_template(&normalized) {
                if let RuntimeExpressionSegment::Expression(expr) = seg {
                    validate_runtime_expression(&expr)?;
                }
            }
        }
        Ok(Self(normalized))
    }

    /// Parses and validates a required runtime expression or template.
    ///
    /// This rejects literals and requires at least one valid runtime expression.
    pub fn parse_expression(raw: &str) -> AppResult<Self> {
        let normalized = normalize_runtime_expression(raw);
        if normalized.starts_with('$') {
            validate_runtime_expression(&normalized)?;
            return Ok(Self(normalized));
        }

        let mut has_expr = false;
        for seg in split_runtime_expression_template(&normalized) {
            if let RuntimeExpressionSegment::Expression(expr) = seg {
                has_expr = true;
                validate_runtime_expression(&expr)?;
            }
        }

        if !has_expr {
            return Err(AppError::General(format!(
                "Runtime expression must include a '$' expression: '{}'",
                raw
            )));
        }

        Ok(Self(normalized))
    }

    /// Returns the raw expression string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Heuristic check if the string looks like an expression (starts with `$`).
    pub fn is_expression(&self) -> bool {
        if self.0.starts_with('$') {
            return true;
        }
        contains_runtime_expression(&self.0)
    }
}

impl fmt::Debug for RuntimeExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RuntimeExpression({:?})", self.0)
    }
}

impl fmt::Display for RuntimeExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Represents a parsed API route.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedRoute {
    /// The URL path (e.g. "/users/{id}") or Webhook Name (e.g. "userCreated").
    pub path: String,
    /// A short summary of what the operation does.
    ///
    /// This is the effective summary used for docs, with the path-level summary
    /// as a fallback when the operation summary is absent.
    pub summary: Option<String>,
    /// A verbose description of the operation behavior.
    ///
    /// This is the effective description used for docs, with the path-level
    /// description as a fallback when the operation description is absent.
    pub description: Option<String>,
    /// Optional path-level summary from the Path Item Object.
    pub path_summary: Option<String>,
    /// Optional path-level description from the Path Item Object.
    pub path_description: Option<String>,
    /// Optional operation-level summary from the Operation Object.
    pub operation_summary: Option<String>,
    /// Optional operation-level description from the Operation Object.
    pub operation_description: Option<String>,
    /// Specification extensions (`x-...`) attached to the Path Item Object.
    pub path_extensions: BTreeMap<String, JsonValue>,
    /// The base URL path derived from the `servers` block (e.g. "/api/v1").
    pub base_path: Option<String>,
    /// Optional path-level server overrides (full Server Objects).
    pub path_servers: Option<Vec<ParsedServer>>,
    /// Optional per-operation server overrides (full Server Objects).
    pub servers_override: Option<Vec<ParsedServer>>,
    /// HTTP Method: "GET", "POST", etc.
    pub method: String,
    /// Rust handler name, typically snake_case of operationId
    pub handler_name: String,
    /// Original operationId value (case-sensitive) when provided in the OpenAPI document.
    pub operation_id: Option<String>,
    /// Route parameters (path, query, header, cookie)
    pub params: Vec<RouteParam>,
    /// Parameters defined at the Path Item level.
    ///
    /// These are preserved separately for OpenAPI round-tripping and may be
    /// overridden by operation-level parameters with the same name+location.
    pub path_params: Vec<RouteParam>,
    /// Request body definition (if any).
    pub request_body: Option<RequestBodyDefinition>,
    /// Raw request body payload from the OpenAPI document, for round-trip preservation.
    ///
    /// This stores the full `requestBody` object (including all media types and examples)
    /// when parsing OpenAPI 3.x documents. It is omitted for Swagger 2.0 inputs.
    pub raw_request_body: Option<JsonValue>,
    /// Security requirements grouped by alternative sets (OR semantics).
    pub security: Vec<SecurityRequirementGroup>,
    /// Whether the Operation Object explicitly declared `security` (including an empty array).
    ///
    /// When false, `security` may still be populated from a top-level default.
    pub security_defined: bool,
    /// The classification of this route.
    pub kind: RouteKind,
    /// Tags associated with the operation (used for grouping/module organization).
    pub tags: Vec<String>,
    /// The Rust type name of the selected success response (e.g. `UserResponse`, `Vec<User>`).
    /// Only present if a concrete response body schema is defined inline.
    pub response_type: Option<String>,
    /// The response status code or range selected for this route (e.g. "200", "201", "2XX", "default").
    pub response_status: Option<String>,
    /// The response summary associated with the selected response.
    pub response_summary: Option<String>,
    /// The description associated with the selected response.
    pub response_description: Option<String>,
    /// The media type selected for the response body (e.g. "application/json", "text/plain").
    pub response_media_type: Option<String>,
    /// Example payload for the selected response (data or serialized).
    pub response_example: Option<ExampleValue>,
    /// Response headers defined in the operation.
    pub response_headers: Vec<ResponseHeader>,
    /// Raw responses map from the OpenAPI document, for round-trip preservation.
    ///
    /// This stores the full `responses` object (including all status codes and media types)
    /// when parsing OpenAPI 3.x documents. It is omitted for Swagger 2.0 inputs.
    pub raw_responses: Option<JsonValue>,
    /// Response links defined in the operation (HATEOAS).
    pub response_links: Option<Vec<ParsedLink>>,
    /// Callback definitions attached to this route (OAS 3.0+).
    pub callbacks: Vec<ParsedCallback>,
    /// Whether this route is deprecated.
    pub deprecated: bool,
    /// External documentation link.
    pub external_docs: Option<ParsedExternalDocs>,
    /// Specification extensions (`x-...`) attached to the Operation Object.
    pub extensions: BTreeMap<String, JsonValue>,
}

impl ParsedRoute {
    /// Returns the strongly typed [`HttpMethod`] for this route.
    pub fn http_method(&self) -> HttpMethod {
        self.method.parse().unwrap_or(HttpMethod::Get)
    }

    /// Returns the sanitized [`RustIdent`] for this route's handler function.
    pub fn handler_ident(&self) -> RustIdent {
        RustIdent::sanitize_snake(&self.handler_name, "handler")
    }

    /// Returns the validated [`OperationId`] when `operation_id` is present and non-empty.
    pub fn typed_operation_id(&self) -> Option<OperationId> {
        self.operation_id
            .as_deref()
            .and_then(|id| OperationId::parse(id).ok())
    }

    /// Returns the strongly typed [`StatusCodeSpec`] for the selected response.
    pub fn response_status_spec(&self) -> Option<StatusCodeSpec> {
        self.response_status.as_deref().and_then(|s| s.parse().ok())
    }

    /// Returns the structured [`MediaType`] for the selected response body.
    pub fn response_media_type_parsed(&self) -> Option<MediaType> {
        self.response_media_type.as_deref().map(MediaType::parse)
    }

    /// Returns the structured [`RustType`] AST for the selected response body type.
    pub fn response_rust_type(&self) -> Option<RustType> {
        self.response_type.as_deref().map(RustType::parse)
    }

    /// Returns validated [`ExtensionKey`] entries from `extensions`.
    pub fn typed_extensions(&self) -> BTreeMap<ExtensionKey, JsonValue> {
        self.extensions
            .iter()
            .filter_map(|(k, v)| ExtensionKey::parse(k).ok().map(|ek| (ek, v.clone())))
            .collect()
    }
}

/// Represents a header returned in the response.
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseHeader {
    /// Name of the header (e.g., "X-Rate-Limit-Limit").
    pub name: String,
    /// Description of the header.
    pub description: Option<String>,
    /// Whether this header is required.
    pub required: bool,
    /// Whether this header is deprecated.
    pub deprecated: bool,
    /// Serialization style for schema-based headers (must be `simple` when set).
    pub style: Option<ParamStyle>,
    /// Explicit explode setting for schema-based headers.
    pub explode: Option<bool>,
    /// Rust type of the header value (e.g., "i32", "String").
    pub ty: String,
    /// Media type used when this header is defined via `content`.
    ///
    /// When set, OpenAPI generation will emit `content` instead of `schema`.
    pub content_media_type: Option<String>,
    /// Optional example value for the header.
    pub example: Option<ExampleValue>,
    /// Specification extensions (`x-...`) attached to the Header Object.
    pub extensions: BTreeMap<String, JsonValue>,
}

impl ResponseHeader {
    /// Returns the structured [`RustType`] AST for this response header's value.
    pub fn rust_type(&self) -> RustType {
        RustType::parse(&self.ty)
    }

    /// Returns validated [`ExtensionKey`] entries from `extensions`.
    pub fn typed_extensions(&self) -> BTreeMap<ExtensionKey, JsonValue> {
        self.extensions
            .iter()
            .filter_map(|(k, v)| ExtensionKey::parse(k).ok().map(|ek| (ek, v.clone())))
            .collect()
    }
}

/// Represents a static link relationship defined in the response.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLink {
    /// Short name of the link (map key).
    pub name: String,
    /// Description of the link.
    pub description: Option<String>,
    /// The name of an existing, resolvable OAS operation (operationId).
    pub operation_id: Option<String>,
    /// A relative or absolute URI reference to an OAS operation.
    pub operation_ref: Option<String>,
    /// Resolved operation reference path for code generation.
    ///
    /// This is derived from `operationId` or `operationRef` during parsing and
    /// must not be emitted back into OpenAPI, preserving the original link definition.
    pub resolved_operation_ref: Option<String>,
    /// Parameters to pass to the linked operation.
    /// Key: Parameter name, Value: literal or runtime expression.
    pub parameters: HashMap<String, LinkParamValue>,
    /// Optional request body to pass to the linked operation.
    pub request_body: Option<LinkRequestBody>,
    /// Optional server URL override for the linked operation.
    pub server: Option<ParsedServer>,
    /// Optional resolved server URL (variables substituted).
    pub server_url: Option<String>,
}

/// Represents a callback definition (outgoing webhook).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCallback {
    /// The callback name (key in the callbacks map).
    pub name: String,
    /// The runtime expression URL (e.g. "{$request.query.callbackUrl}").
    pub expression: RuntimeExpression,
    /// The HTTP method for the callback request.
    pub method: String,
    /// Parameters applicable to this callback operation.
    ///
    /// This list includes operation-level parameters plus any path-item parameters
    /// not overridden at the operation level.
    pub params: Vec<RouteParam>,
    /// Parameters defined at the callback Path Item level.
    ///
    /// These are preserved separately for OpenAPI round-tripping and may be
    /// overridden by operation-level parameters with the same name+location.
    pub path_params: Vec<RouteParam>,
    /// Security requirements for this callback operation (OR semantics across groups).
    ///
    /// When empty, security may still be inherited from a top-level default.
    pub security: Vec<SecurityRequirementGroup>,
    /// Whether the callback Operation Object explicitly declared `security`.
    pub security_defined: bool,
    /// The expected body sent in the callback (if any).
    pub request_body: Option<RequestBodyDefinition>,
    /// The expected response type from the callback receiver.
    pub response_type: Option<String>,
    /// The response status code or range selected for this callback (e.g. "200", "202", "default").
    pub response_status: Option<String>,
    /// The summary associated with the selected callback response.
    pub response_summary: Option<String>,
    /// The description associated with the selected callback response.
    pub response_description: Option<String>,
    /// The media type selected for the callback response body.
    pub response_media_type: Option<String>,
    /// Example payload for the callback response (data or serialized).
    pub response_example: Option<ExampleValue>,
    /// Headers expected in the callback receiver's response.
    pub response_headers: Vec<ResponseHeader>,
}

/// Represents a value used for Link Object parameters.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkParamValue {
    /// A runtime expression value.
    Expression(RuntimeExpression),
    /// A literal JSON value.
    Literal(JsonValue),
}

/// Represents a request body value for a Link Object.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkRequestBody {
    /// A runtime expression value.
    Expression(RuntimeExpression),
    /// A literal JSON value.
    Literal(JsonValue),
}

/// Detailed information about a Security Scheme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecuritySchemeInfo {
    /// The type of scheme (ApiKey, Http, etc.).
    pub kind: SecuritySchemeKind,
    /// The description provided in the spec.
    pub description: Option<String>,
    /// Whether this security scheme is deprecated.
    pub deprecated: bool,
}

/// OAuth2 flow definition (OAS 3.x).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthFlow {
    /// Authorization URL (implicit, authorizationCode).
    pub authorization_url: Option<String>,
    /// Device authorization URL (deviceAuthorization).
    pub device_authorization_url: Option<String>,
    /// Token URL (password, clientCredentials, authorizationCode, deviceAuthorization).
    pub token_url: Option<String>,
    /// Refresh URL (optional).
    pub refresh_url: Option<String>,
    /// Available scopes and their descriptions.
    pub scopes: BTreeMap<String, String>,
}

/// Container for OAuth2 flows (OAS 3.x).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthFlows {
    /// Implicit flow.
    pub implicit: Option<OAuthFlow>,
    /// Resource owner password flow.
    pub password: Option<OAuthFlow>,
    /// Client credentials flow.
    pub client_credentials: Option<OAuthFlow>,
    /// Authorization code flow.
    pub authorization_code: Option<OAuthFlow>,
    /// Device authorization flow (RFC8628).
    pub device_authorization: Option<OAuthFlow>,
}

impl OAuthFlows {
    /// Returns true if no flows are defined.
    pub fn is_empty(&self) -> bool {
        self.implicit.is_none()
            && self.password.is_none()
            && self.client_credentials.is_none()
            && self.authorization_code.is_none()
            && self.device_authorization.is_none()
    }
}

/// Classification of the security scheme logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecuritySchemeKind {
    /// API Key (Header, Query, Cookie).
    ApiKey {
        /// Parameter name.
        name: String,
        /// Location.
        in_loc: ParamSource,
    },
    /// HTTP Authentication (Basic, Bearer, etc.).
    Http {
        /// Scheme (basic, bearer).
        scheme: String,
        /// Format (e.g. JWT).
        bearer_format: Option<String>,
    },
    /// OAuth2 Flows.
    OAuth2 {
        /// OAuth2 flow definitions.
        flows: Box<OAuthFlows>,
        /// Optional OAuth2 authorization server metadata URL (RFC8414).
        oauth2_metadata_url: Option<String>,
    },
    /// OpenID Connect.
    OpenIdConnect {
        /// OpenID Connect discovery URL.
        open_id_connect_url: String,
    },
    /// Mutual TLS.
    MutualTls,
}

/// A single security scheme requirement (AND logic within a group).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityRequirement {
    /// Name of the security scheme in components.
    pub scheme_name: String,
    /// Required scopes (for OAuth2/OIDC).
    pub scopes: Vec<String>,
    /// Resolved scheme details (if available).
    pub scheme: Option<SecuritySchemeInfo>,
}

/// A grouped security requirement object (OR semantics across groups).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityRequirementGroup {
    /// Schemes required together for this alternative.
    pub schemes: Vec<SecurityRequirement>,
}

impl SecurityRequirementGroup {
    /// Creates an empty (anonymous) requirement object: `{}`.
    pub fn anonymous() -> Self {
        Self {
            schemes: Vec::new(),
        }
    }

    /// Returns true if this group represents anonymous access.
    pub fn is_anonymous(&self) -> bool {
        self.schemes.is_empty()
    }
}

/// Definition of a request body type and format.
#[derive(Debug, Clone, PartialEq)]
pub struct RequestBodyDefinition {
    /// The Rust type name (e.g. "CreateUserRequest").
    pub ty: String,
    /// Optional description for the request body.
    pub description: Option<String>,
    /// The selected media type (e.g. "application/json", "application/xml").
    pub media_type: String,
    /// The format of the body (JSON, Form, etc.).
    pub format: BodyFormat,
    /// Whether the request body is required by the operation.
    pub required: bool,
    /// Multipart/Form Encoding details.
    pub encoding: Option<HashMap<String, EncodingInfo>>,
    /// Positional encodings for multipart payloads (prefix items).
    pub prefix_encoding: Option<Vec<EncodingInfo>>,
    /// Positional encoding for remaining multipart items.
    pub item_encoding: Option<EncodingInfo>,
    /// Optional example payload for the request body.
    pub example: Option<ExampleValue>,
}

impl RequestBodyDefinition {
    /// Returns the structured [`RustType`] AST for the request body payload.
    pub fn rust_type(&self) -> RustType {
        RustType::parse(&self.ty)
    }

    /// Returns the structured [`MediaType`] for the request body.
    pub fn parsed_media_type(&self) -> MediaType {
        MediaType::parse(&self.media_type)
    }
}

/// Encoding details for a specific property.
#[derive(Debug, Clone, PartialEq)]
pub struct EncodingInfo {
    /// Content-Type.
    pub content_type: Option<String>,
    /// Headers map.
    pub headers: HashMap<String, String>,
    /// RFC6570-style serialization for form-like encodings.
    pub style: Option<ParamStyle>,
    /// Explicit explode override (if set in the Encoding Object).
    pub explode: Option<bool>,
    /// Allow reserved characters (RFC3986) without percent-encoding.
    pub allow_reserved: Option<bool>,
    /// Nested encoding definitions keyed by property name.
    pub encoding: Option<HashMap<String, EncodingInfo>>,
    /// Positional encoding definitions for multipart sequences.
    pub prefix_encoding: Option<Vec<EncodingInfo>>,
    /// Encoding definition for remaining multipart items.
    pub item_encoding: Option<Box<EncodingInfo>>,
}

/// Supported body content types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyFormat {
    /// application/json
    Json,
    /// application/x-www-form-urlencoded
    Form,
    /// multipart/form-data or multipart/mixed
    Multipart,
    /// text/plain and other text/* media types.
    Text,
    /// Binary payloads (e.g., application/octet-stream, image/*).
    Binary,
}

/// Normalized media type classification for `content`-based parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentMediaType {
    /// `application/x-www-form-urlencoded`
    FormUrlEncoded,
    /// `application/json` or `+json` vendor media types.
    Json,
    /// Any other media type (stored as provided).
    Other(String),
}

/// Indicates whether an example is raw data or already serialized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExampleKind {
    /// Example represents data that still needs serialization.
    Data,
    /// Example is already serialized (e.g., `serializedValue`).
    Serialized,
    /// Example is stored externally via `externalValue`.
    External,
}

/// Example payload with explicit serialization semantics.
#[derive(Debug, Clone, PartialEq)]
pub struct ExampleValue {
    /// Whether the example is data or already serialized.
    pub kind: ExampleKind,
    /// The example payload value.
    pub value: JsonValue,
    /// Optional short summary of the example.
    pub summary: Option<String>,
    /// Optional long description of the example.
    pub description: Option<String>,
}

impl ExampleValue {
    /// Executes `new` (new).
    ///
    /// # Arguments
    /// * `kind` - Input `kind` parameter.
    /// * `value` - Input `value` parameter.
    /// * `summary` - Input `summary` parameter.
    /// * `description` - Input `description` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    fn new(
        kind: ExampleKind,
        value: JsonValue,
        summary: Option<String>,
        description: Option<String>,
    ) -> Self {
        Self {
            kind,
            value,
            summary,
            description,
        }
    }

    /// Creates a data example.
    pub fn data(value: JsonValue) -> Self {
        Self::new(ExampleKind::Data, value, None, None)
    }

    /// Creates a serialized example.
    pub fn serialized(value: JsonValue) -> Self {
        Self::new(ExampleKind::Serialized, value, None, None)
    }

    /// Creates an external example.
    pub fn external(value: JsonValue) -> Self {
        Self::new(ExampleKind::External, value, None, None)
    }

    /// Creates a data example with optional summary/description metadata.
    pub fn data_with_meta(
        value: JsonValue,
        summary: Option<String>,
        description: Option<String>,
    ) -> Self {
        Self::new(ExampleKind::Data, value, summary, description)
    }

    /// Creates a serialized example with optional summary/description metadata.
    pub fn serialized_with_meta(
        value: JsonValue,
        summary: Option<String>,
        description: Option<String>,
    ) -> Self {
        Self::new(ExampleKind::Serialized, value, summary, description)
    }

    /// Creates an external example with optional summary/description metadata.
    pub fn external_with_meta(
        value: JsonValue,
        summary: Option<String>,
        description: Option<String>,
    ) -> Self {
        Self::new(ExampleKind::External, value, summary, description)
    }

    /// Applies Reference Object summary/description overrides, replacing existing metadata
    /// when provided.
    pub fn with_overrides(mut self, summary: Option<String>, description: Option<String>) -> Self {
        if summary.is_some() {
            self.summary = summary;
        }
        if description.is_some() {
            self.description = description;
        }
        self
    }

    /// Returns true if the example is already serialized.
    pub fn is_serialized(&self) -> bool {
        matches!(self.kind, ExampleKind::Serialized | ExampleKind::External)
    }

    /// Returns true if the example uses `externalValue`.
    pub fn is_external(&self) -> bool {
        matches!(self.kind, ExampleKind::External)
    }
}

impl ContentMediaType {
    /// Normalizes a media type string into a structured variant.
    pub fn from_media_type(media_type: &str) -> Self {
        let normalized = media_type
            .split(';')
            .next()
            .unwrap_or(media_type)
            .trim()
            .to_ascii_lowercase();

        if normalized == "application/x-www-form-urlencoded" {
            return ContentMediaType::FormUrlEncoded;
        }

        if normalized == "application/json"
            || normalized == "application/*+json"
            || normalized.ends_with("+json")
        {
            return ContentMediaType::Json;
        }

        ContentMediaType::Other(media_type.to_string())
    }

    /// Returns a canonical string representation where possible.
    pub fn as_str(&self) -> &str {
        match self {
            ContentMediaType::FormUrlEncoded => "application/x-www-form-urlencoded",
            ContentMediaType::Json => "application/json",
            ContentMediaType::Other(val) => val.as_str(),
        }
    }
}

/// Parameter serialization style.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ParamStyle {
    /// `matrix`
    Matrix,
    /// `label`
    Label,
    /// `form`
    Form,
    /// `cookie`
    Cookie,
    /// `simple`
    #[default]
    Simple,
    /// `spaceDelimited`
    SpaceDelimited,
    /// `pipeDelimited`
    PipeDelimited,
    /// `deepObject`
    DeepObject,
}

/// Represents a parameter in a route.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteParam {
    /// Parameter name in the source.
    pub name: String,
    /// Description of the parameter.
    pub description: Option<String>,
    /// Location.
    pub source: ParamSource,
    /// Rust type.
    pub ty: String,
    /// Media type used when `content` is specified (querystring or complex parameters).
    pub content_media_type: Option<ContentMediaType>,
    /// Serialization style.
    pub style: Option<ParamStyle>,
    /// Explode modifier.
    pub explode: bool,
    /// Allow reserved characters.
    pub allow_reserved: bool,
    /// Whether this parameter is deprecated.
    pub deprecated: bool,
    /// Whether this query parameter allows an empty value.
    ///
    /// This is only valid for `in: query` parameters (deprecated in OAS 3.2).
    pub allow_empty_value: bool,
    /// Optional example value for this parameter.
    pub example: Option<ExampleValue>,
    /// Raw schema object for lossless keyword passthrough (OAS 3.1+).
    ///
    /// When present, unknown JSON Schema keywords are merged into the generated schema.
    pub raw_schema: Option<JsonValue>,
    /// Specification extensions (`x-...`) attached to the Parameter Object.
    pub extensions: BTreeMap<String, JsonValue>,
}

impl RouteParam {
    /// Returns the structured [`RustType`] AST for this parameter.
    pub fn rust_type(&self) -> RustType {
        RustType::parse(&self.ty)
    }

    /// Returns validated [`ExtensionKey`] entries from `extensions`.
    pub fn typed_extensions(&self) -> BTreeMap<ExtensionKey, JsonValue> {
        self.extensions
            .iter()
            .filter_map(|(k, v)| ExtensionKey::parse(k).ok().map(|ek| (ek, v.clone())))
            .collect()
    }
}

/// The source location of a parameter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParamSource {
    /// Path.
    Path,
    /// Query.
    Query,
    /// Query String (OAS 3.2).
    QueryString,
    /// Header.
    Header,
    /// Cookie.
    Cookie,
    /// Form Data (OAS 2.0).
    FormData,
    /// Body (OAS 2.0).
    Body,
}

/// Executes `normalize_runtime_expression` (normalize runtime expression).
///
/// # Arguments
/// * `raw` - Input `raw` parameter.
///
/// # Returns
/// Resulting `String` value.
fn normalize_runtime_expression(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(inner) = trimmed.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        let inner = inner.trim();
        if inner.starts_with('$') {
            return inner.to_string();
        }
    }

    trimmed.to_string()
}

/// A parsed segment of a runtime expression template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeExpressionSegment {
    /// Literal text segment.
    Literal(String),
    /// Embedded runtime expression (without surrounding braces).
    Expression(String),
}

/// Splits a runtime expression template into literal and expression segments.
///
/// Expressions are identified as `{...}` segments whose trimmed inner text starts with `$`.
/// Non-expression braces are preserved as literals.
pub fn split_runtime_expression_template(raw: &str) -> Vec<RuntimeExpressionSegment> {
    let mut segments = Vec::new();
    let mut buf = String::new();
    let mut chars = raw.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            let mut inner = String::new();
            let mut found_end = false;
            for n in chars.by_ref() {
                if n == '}' {
                    found_end = true;
                    break;
                }
                inner.push(n);
            }

            if found_end {
                let trimmed = inner.trim();
                if trimmed.starts_with('$') {
                    if !buf.is_empty() {
                        segments.push(RuntimeExpressionSegment::Literal(std::mem::take(&mut buf)));
                    }
                    segments.push(RuntimeExpressionSegment::Expression(trimmed.to_string()));
                } else {
                    buf.push('{');
                    buf.push_str(&inner);
                    buf.push('}');
                }
            } else {
                buf.push('{');
                buf.push_str(&inner);
            }
        } else {
            buf.push(c);
        }
    }

    if !buf.is_empty() {
        segments.push(RuntimeExpressionSegment::Literal(buf));
    }

    segments
}

/// Executes `contains_runtime_expression` (contains runtime expression).
///
/// # Arguments
/// * `raw` - Input `raw` parameter.
///
/// # Returns
/// Resulting `bool` value.
fn contains_runtime_expression(raw: &str) -> bool {
    split_runtime_expression_template(raw)
        .iter()
        .any(|seg| matches!(seg, RuntimeExpressionSegment::Expression(_)))
}

/// Executes `validate_runtime_expression` (validate runtime expression).
///
/// # Arguments
/// * `expr` - Input `expr` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_runtime_expression(expr: &str) -> AppResult<()> {
    match expr {
        "$url" | "$method" | "$statusCode" | "$self" => return Ok(()),
        _ => {}
    }

    if let Some(rest) = expr.strip_prefix("$request.") {
        return validate_runtime_source(rest);
    }

    if let Some(rest) = expr.strip_prefix("$response.") {
        return validate_runtime_source(rest);
    }

    if let Some(rest) = expr.strip_prefix("$message.") {
        return validate_message_source(rest);
    }

    if let Some(rest) = expr
        .strip_prefix("$inputs.")
        .or_else(|| expr.strip_prefix("$outputs."))
    {
        return validate_named_pointer_tail(rest, "inputs/outputs");
    }

    if let Some(rest) = expr
        .strip_prefix("$steps.")
        .or_else(|| expr.strip_prefix("$workflows."))
        .or_else(|| expr.strip_prefix("$sourceDescriptions."))
    {
        return validate_scoped_expression_tail(rest);
    }

    if let Some(rest) = expr.strip_prefix("$components.") {
        return validate_components_tail(rest);
    }

    Err(AppError::General(format!(
        "Invalid runtime expression: '{}'",
        expr
    )))
}

/// Executes `validate_message_source` (validate message source).
///
/// # Arguments
/// * `source` - Input `source` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_message_source(source: &str) -> AppResult<()> {
    if let Some(tail) = source.strip_prefix("header.") {
        return validate_header_token(tail);
    }
    if let Some(tail) = source
        .strip_prefix("header")
        .or_else(|| source.strip_prefix("payload"))
    {
        return validate_body_reference(tail);
    }
    Err(AppError::General(format!(
        "Invalid message runtime expression source: '{}'",
        source
    )))
}

/// Executes `validate_named_pointer_tail` (validate named pointer tail).
///
/// # Arguments
/// * `tail` - Input `tail` parameter.
/// * `kind` - Input `kind` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_named_pointer_tail(tail: &str, kind: &str) -> AppResult<()> {
    let (head, ptr_opt) = match tail.split_once('#') {
        Some((h, p)) => (h, Some(p)),
        None => (tail, None),
    };
    if head.is_empty() || head.split('.').any(|seg| seg.is_empty()) {
        return Err(AppError::General(format!(
            "Runtime expression {} name must not be empty",
            kind
        )));
    }
    if let Some(ptr) = ptr_opt {
        validate_json_pointer(ptr)?;
    }
    Ok(())
}

/// Executes `validate_scoped_expression_tail` (validate scoped expression tail).
///
/// # Arguments
/// * `tail` - Input `tail` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_scoped_expression_tail(tail: &str) -> AppResult<()> {
    let (scope_id, rest) = tail.split_once('.').ok_or_else(|| {
        AppError::General(format!(
            "Scoped runtime expression requires '<id>.<member>': '{}'",
            tail
        ))
    })?;
    if scope_id.is_empty() {
        return Err(AppError::General(
            "Scoped runtime expression identifier must not be empty".into(),
        ));
    }
    validate_named_pointer_tail(rest, "scoped member")
}

/// Executes `validate_components_tail` (validate components tail).
///
/// # Arguments
/// * `tail` - Input `tail` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_components_tail(tail: &str) -> AppResult<()> {
    let (section, name) = tail.split_once('.').ok_or_else(|| {
        AppError::General(format!(
            "Components runtime expression requires '<section>.<name>': '{}'",
            tail
        ))
    })?;
    if !matches!(
        section,
        "parameters" | "successActions" | "failureActions" | "inputs"
    ) || name.is_empty()
        || name.contains('.')
    {
        return Err(AppError::General(format!(
            "Invalid components runtime expression: '$components.{}'",
            tail
        )));
    }
    Ok(())
}

/// Executes `validate_runtime_source` (validate runtime source).
///
/// # Arguments
/// * `source` - Input `source` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_runtime_source(source: &str) -> AppResult<()> {
    if let Some(header) = source.strip_prefix("header.") {
        return validate_header_token(header);
    }
    if let Some(query) = source.strip_prefix("query.") {
        return validate_name(query, "query");
    }
    if let Some(path) = source.strip_prefix("path.") {
        return validate_name(path, "path");
    }
    if let Some(body) = source.strip_prefix("body") {
        return validate_body_reference(body);
    }

    Err(AppError::General(format!(
        "Invalid runtime expression source: '{}'",
        source
    )))
}

/// Executes `validate_header_token` (validate header token).
///
/// # Arguments
/// * `token` - Input `token` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_header_token(token: &str) -> AppResult<()> {
    if token.is_empty() {
        return Err(AppError::General(
            "Runtime expression header token must not be empty".into(),
        ));
    }

    if token.chars().all(is_tchar) {
        Ok(())
    } else {
        Err(AppError::General(format!(
            "Invalid header token in runtime expression: '{}'",
            token
        )))
    }
}

/// Executes `validate_name` (validate name).
///
/// # Arguments
/// * `name` - Input `name` parameter.
/// * `kind` - Input `kind` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_name(name: &str, kind: &str) -> AppResult<()> {
    if name.is_empty() {
        Err(AppError::General(format!(
            "Runtime expression {} name must not be empty",
            kind
        )))
    } else {
        Ok(())
    }
}

/// Executes `validate_body_reference` (validate body reference).
///
/// # Arguments
/// * `tail` - Input `tail` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_body_reference(tail: &str) -> AppResult<()> {
    if tail.is_empty() {
        return Ok(());
    }

    if let Some(ptr) = tail.strip_prefix('#') {
        return validate_json_pointer(ptr);
    }

    Err(AppError::General(format!(
        "Invalid body reference in runtime expression: 'body{}'",
        tail
    )))
}

/// Executes `validate_json_pointer` (validate json pointer).
///
/// # Arguments
/// * `ptr` - Input `ptr` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_json_pointer(ptr: &str) -> AppResult<()> {
    if ptr.is_empty() {
        return Ok(());
    }

    if !ptr.starts_with('/') {
        return Err(AppError::General(format!(
            "JSON Pointer in runtime expression must start with '/': '{}'",
            ptr
        )));
    }

    for segment in ptr.split('/').skip(1) {
        if !validate_pointer_segment(segment) {
            return Err(AppError::General(format!(
                "Invalid JSON Pointer segment in runtime expression: '{}'",
                segment
            )));
        }
    }

    Ok(())
}

/// Executes `validate_pointer_segment` (validate pointer segment).
///
/// # Arguments
/// * `segment` - Input `segment` parameter.
///
/// # Returns
/// Resulting `bool` value.
fn validate_pointer_segment(segment: &str) -> bool {
    let mut chars = segment.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '~' {
            match chars.next() {
                Some('0') | Some('1') => continue,
                _ => return false,
            }
        }
    }
    true
}

/// Executes `is_tchar` (is tchar).
///
/// # Arguments
/// * `c` - Input `c` parameter.
///
/// # Returns
/// Resulting `bool` value.
fn is_tchar(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '!' | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '.'
                | '^'
                | '_'
                | '`'
                | '|'
                | '~'
        )
}

/// Validated custom HTTP method token (RFC 9110 `tchar` token).
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct CustomHttpMethod(String);

impl CustomHttpMethod {
    /// Parses and validates a custom HTTP method token.
    ///
    /// # Arguments
    /// * `raw` - Raw method string.
    ///
    /// # Returns
    /// A validated [`CustomHttpMethod`].
    ///
    /// # Errors
    /// Returns [`AppError`] if the token is empty or contains non-token characters.
    pub fn parse(raw: &str) -> AppResult<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || !trimmed.chars().all(is_tchar) {
            return Err(AppError::General(format!(
                "Invalid HTTP method token: '{}'",
                raw
            )));
        }
        Ok(Self(trimmed.to_ascii_uppercase()))
    }

    /// Returns the method token string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Strongly typed HTTP method representation supporting standard OAS 3.2 methods
/// (`GET`..`QUERY`) and `additionalOperations` custom methods.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum HttpMethod {
    /// HTTP `GET`.
    Get,
    /// HTTP `POST`.
    Post,
    /// HTTP `PUT`.
    Put,
    /// HTTP `DELETE`.
    Delete,
    /// HTTP `PATCH`.
    Patch,
    /// HTTP `OPTIONS`.
    Options,
    /// HTTP `HEAD`.
    Head,
    /// HTTP `TRACE`.
    Trace,
    /// HTTP `QUERY` (OAS 3.2).
    Query,
    /// Custom HTTP method token (`additionalOperations`).
    Custom(CustomHttpMethod),
}

impl HttpMethod {
    /// Returns the uppercase HTTP method string.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
            Self::Options => "OPTIONS",
            Self::Head => "HEAD",
            Self::Trace => "TRACE",
            Self::Query => "QUERY",
            Self::Custom(c) => c.as_str(),
        }
    }

    /// Returns the lowercase method name suitable for OpenAPI keys and framework builders.
    pub fn to_lowercase_str(&self) -> String {
        self.as_str().to_ascii_lowercase()
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for HttpMethod {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "GET" => Ok(Self::Get),
            "POST" => Ok(Self::Post),
            "PUT" => Ok(Self::Put),
            "DELETE" => Ok(Self::Delete),
            "PATCH" => Ok(Self::Patch),
            "OPTIONS" => Ok(Self::Options),
            "HEAD" => Ok(Self::Head),
            "TRACE" => Ok(Self::Trace),
            "QUERY" => Ok(Self::Query),
            other => Ok(Self::Custom(CustomHttpMethod::parse(other)?)),
        }
    }
}

/// HTTP response status range (`1XX`..`5XX`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum StatusRange {
    /// Informational `1XX`.
    Info1xx,
    /// Successful `2XX`.
    Success2xx,
    /// Redirection `3XX`.
    Redirect3xx,
    /// Client Error `4XX`.
    ClientError4xx,
    /// Server Error `5XX`.
    ServerError5xx,
}

/// Strongly typed OpenAPI response status code specification (`200`, `2XX`, or `default`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum StatusCodeSpec {
    /// Explicit 3-digit HTTP status code (`100`..=`599`).
    Code(u16),
    /// Wildcard status range (`1XX`..`5XX`).
    Range(StatusRange),
    /// OpenAPI `default` fallback response.
    Default,
}

impl StatusCodeSpec {
    /// Returns a concrete HTTP status code suitable for unit test assertions.
    pub fn representative_code(&self) -> u16 {
        match self {
            Self::Code(c) => *c,
            Self::Range(StatusRange::Info1xx) => 100,
            Self::Range(StatusRange::Success2xx) | Self::Default => 200,
            Self::Range(StatusRange::Redirect3xx) => 302,
            Self::Range(StatusRange::ClientError4xx) => 400,
            Self::Range(StatusRange::ServerError5xx) => 500,
        }
    }
}

impl fmt::Display for StatusCodeSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Code(c) => write!(f, "{}", c),
            Self::Range(StatusRange::Info1xx) => f.write_str("1XX"),
            Self::Range(StatusRange::Success2xx) => f.write_str("2XX"),
            Self::Range(StatusRange::Redirect3xx) => f.write_str("3XX"),
            Self::Range(StatusRange::ClientError4xx) => f.write_str("4XX"),
            Self::Range(StatusRange::ServerError5xx) => f.write_str("5XX"),
            Self::Default => f.write_str("default"),
        }
    }
}

impl std::str::FromStr for StatusCodeSpec {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.eq_ignore_ascii_case("default") {
            return Ok(Self::Default);
        }
        match trimmed.to_ascii_uppercase().as_str() {
            "1XX" => return Ok(Self::Range(StatusRange::Info1xx)),
            "2XX" => return Ok(Self::Range(StatusRange::Success2xx)),
            "3XX" => return Ok(Self::Range(StatusRange::Redirect3xx)),
            "4XX" => return Ok(Self::Range(StatusRange::ClientError4xx)),
            "5XX" => return Ok(Self::Range(StatusRange::ServerError5xx)),
            _ => {}
        }
        if trimmed.len() == 3 {
            if let Ok(code) = trimmed.parse::<u16>() {
                if (100..=599).contains(&code) {
                    return Ok(Self::Code(code));
                }
            }
        }
        Err(AppError::General(format!(
            "Invalid OpenAPI status code specification: '{}'",
            s
        )))
    }
}

/// Structured MIME / Media Type representation with type, subtype, `+suffix`, and parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaType {
    /// Top-level type (e.g. `"application"`, `"text"`, `"*"`).
    pub top_type: String,
    /// Subtype without parameters (e.g. `"json"`, `"problem+json"`, `"*"`).
    pub subtype: String,
    /// Optional structured syntax suffix without leading `+` (e.g. `"json"`, `"xml"`, `"jsonl"`).
    pub suffix: Option<String>,
    /// Optional media type parameters (`charset`, `boundary`, etc.).
    pub parameters: BTreeMap<String, String>,
}

impl MediaType {
    /// Parses a media type string into its structured components.
    ///
    /// # Arguments
    /// * `raw` - Raw media type string (e.g. `"Application/Problem+JSON; charset=utf-8"`).
    pub fn parse(raw: &str) -> Self {
        let mut segments = raw.split(';');
        let ess = segments.next().unwrap_or(raw).trim().to_ascii_lowercase();
        let (top_type, subtype) = match ess.split_once('/') {
            Some((t, s)) => (t.trim().to_string(), s.trim().to_string()),
            None => (ess, "*".to_string()),
        };
        let suffix = subtype
            .rsplit_once('+')
            .map(|(_, suf)| suf.trim().to_string())
            .filter(|s| !s.is_empty());

        let mut parameters = BTreeMap::new();
        for seg in segments {
            if let Some((k, v)) = seg.split_once('=') {
                parameters.insert(
                    k.trim().to_ascii_lowercase(),
                    v.trim().trim_matches('"').to_string(),
                );
            }
        }

        Self {
            top_type,
            subtype,
            suffix,
            parameters,
        }
    }

    /// Returns the normalized `type/subtype` essence in lowercase without parameters.
    pub fn essence(&self) -> String {
        format!("{}/{}", self.top_type, self.subtype)
    }

    /// Normalizes a raw media type string to its lowercase `type/subtype` essence.
    pub fn normalize(raw: &str) -> String {
        let ess = raw.split(';').next().unwrap_or(raw).trim();
        ess.to_ascii_lowercase()
    }

    /// Checks whether this media type represents a sequential JSON stream
    /// (`application/jsonl`, `application/x-ndjson`, `application/json-seq`, `+jsonl`, etc.).
    pub fn is_sequential_json(&self) -> bool {
        let ess = self.essence();
        matches!(
            ess.as_str(),
            "application/jsonl"
                | "application/x-ndjson"
                | "application/json-seq"
                | "application/geo+json-seq"
        ) || matches!(
            self.suffix.as_deref(),
            Some("jsonl" | "ndjson" | "json-seq")
        )
    }

    /// Checks whether this media type represents JSON or a `+json` structured syntax.
    pub fn is_json(&self) -> bool {
        let ess = self.essence();
        ess == "application/json"
            || ess == "application/*+json"
            || self.suffix.as_deref() == Some("json")
            || self.is_sequential_json()
    }

    /// Checks whether this media type represents text or XML.
    pub fn is_text(&self) -> bool {
        let ess = self.essence();
        self.top_type == "text" || ess == "application/xml" || self.suffix.as_deref() == Some("xml")
    }

    /// Checks whether this media type represents `text/event-stream`.
    pub fn is_event_stream(&self) -> bool {
        self.top_type == "text" && self.subtype == "event-stream"
    }

    /// Checks whether this media type represents `application/x-www-form-urlencoded`.
    pub fn is_form_urlencoded(&self) -> bool {
        self.top_type == "application" && self.subtype == "x-www-form-urlencoded"
    }

    /// Checks whether this media type is `multipart/*`.
    pub fn is_multipart(&self) -> bool {
        self.top_type == "multipart"
    }
}

/// Validated Rust identifier newtype centralizing keyword escaping and identifier sanitization.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct RustIdent(String);

impl RustIdent {
    /// Checks whether `ident` is a strict or reserved Rust keyword.
    pub fn is_keyword(ident: &str) -> bool {
        matches!(
            ident,
            "as" | "break"
                | "const"
                | "continue"
                | "crate"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "type"
                | "unsafe"
                | "use"
                | "where"
                | "while"
                | "async"
                | "await"
                | "dyn"
                | "abstract"
                | "become"
                | "box"
                | "do"
                | "final"
                | "macro"
                | "override"
                | "priv"
                | "typeof"
                | "unsized"
                | "virtual"
                | "yield"
                | "try"
        )
    }

    /// Sanitizes an arbitrary string into a valid `snake_case` Rust identifier.
    pub fn sanitize_snake(raw: &str, fallback: &str) -> Self {
        let mut out = String::new();
        let mut prev_underscore = false;

        for ch in raw.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
                prev_underscore = false;
            } else if !prev_underscore && !out.is_empty() {
                out.push('_');
                prev_underscore = true;
            }
        }

        while out.ends_with('_') {
            out.pop();
        }

        if out.is_empty() {
            out.push_str(fallback);
        }

        if out
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            out.insert(0, '_');
        }

        if Self::is_keyword(&out) {
            out.push('_');
        }

        Self(out)
    }

    /// Sanitizes an arbitrary string into a valid `PascalCase` Rust type or enum variant identifier.
    pub fn sanitize_pascal(raw: &str, fallback: &str) -> Self {
        let mut out = String::new();
        let mut capitalize = true;

        for ch in raw.chars() {
            if ch.is_ascii_alphanumeric() {
                if capitalize {
                    out.push(ch.to_ascii_uppercase());
                    capitalize = false;
                } else {
                    out.push(ch);
                }
            } else {
                capitalize = true;
            }
        }

        if out.is_empty() {
            out.push_str(fallback);
        }

        if out
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            out = format!("{}{}", fallback, out);
        }

        if Self::is_keyword(&out) {
            out.push_str(fallback);
        }

        Self(out)
    }

    /// Returns the sanitized identifier string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the wrapper and returns the inner `String`.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for RustIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Validated OpenAPI `operationId` newtype.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct OperationId(String);

impl OperationId {
    /// Constructs a validated `OperationId`, rejecting empty or whitespace-only values.
    pub fn parse(raw: impl Into<String>) -> AppResult<Self> {
        let s = raw.into();
        if s.trim().is_empty() {
            return Err(AppError::General(
                "operationId must not be empty".to_string(),
            ));
        }
        Ok(Self(s))
    }

    /// Converts the operation ID into a sanitized `snake_case` handler [`RustIdent`].
    pub fn to_handler_ident(&self) -> RustIdent {
        RustIdent::sanitize_snake(&self.0, "operation")
    }

    /// Returns the raw `operationId` string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validated OpenAPI specification extension key (`x-...`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct ExtensionKey(String);

impl ExtensionKey {
    /// Constructs a validated `ExtensionKey` enforcing the `x-` prefix.
    pub fn parse(raw: &str) -> AppResult<Self> {
        if !raw.starts_with("x-") || raw.len() <= 2 {
            return Err(AppError::General(format!(
                "Specification extension key '{}' must start with 'x-'",
                raw
            )));
        }
        Ok(Self(raw.to_string()))
    }

    /// Returns the extension key string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> serde::Deserialize<'de> for ExtensionKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Primitive scalar Rust types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveType {
    /// `bool`.
    Bool,
    /// `i8`.
    I8,
    /// `i16`.
    I16,
    /// `i32`.
    I32,
    /// `i64`.
    I64,
    /// `i128`.
    I128,
    /// `isize`.
    Isize,
    /// `u8`.
    U8,
    /// `u16`.
    U16,
    /// `u32`.
    U32,
    /// `u64`.
    U64,
    /// `u128`.
    U128,
    /// `usize`.
    Usize,
    /// `f32`.
    F32,
    /// `f64`.
    F64,
}

/// Structured domain AST for Rust types, replacing fragile substring checks
/// (`ty.contains("u8")`, `ty.starts_with("Option<")`, `ty.contains("Uuid")`, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustType {
    /// Scalar primitive type (`i32`, `u8`, `f64`, `bool`, etc.).
    Primitive(PrimitiveType),
    /// `String` or `&str`.
    String,
    /// `uuid::Uuid`.
    Uuid,
    /// `chrono::DateTime<Utc>`.
    DateTime,
    /// `chrono::NaiveDate`.
    NaiveDate,
    /// `chrono::NaiveDateTime`.
    NaiveDateTime,
    /// `bytes::Bytes`, `Vec<u8>`, or `&[u8]`.
    Bytes,
    /// `serde_json::Value`.
    Value,
    /// `Option<T>`.
    Option(Box<RustType>),
    /// `Vec<T>`.
    Vec(Box<RustType>),
    /// `Secret<T>`.
    Secret(Box<RustType>),
    /// `HashMap<K, V>`.
    HashMap(Box<RustType>, Box<RustType>),
    /// `BTreeMap<K, V>`.
    BTreeMap(Box<RustType>, Box<RustType>),
    /// Tuple `(T1, T2, ...)`.
    Tuple(Vec<RustType>),
    /// Custom struct or enum identifier.
    Custom(RustIdent),
}

impl RustType {
    /// Parses a Rust type string into a structured [`RustType`] AST.
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim().trim_start_matches('&').trim();
        if trimmed == "[u8]" {
            return Self::Bytes;
        }
        if let Some(inner) = trimmed
            .strip_prefix("Option<")
            .and_then(|s| s.strip_suffix('>'))
        {
            return Self::Option(Box::new(Self::parse(inner)));
        }
        if let Some(inner) = trimmed
            .strip_prefix("Vec<")
            .and_then(|s| s.strip_suffix('>'))
        {
            let parsed_inner = Self::parse(inner);
            if parsed_inner == Self::Primitive(PrimitiveType::U8) {
                return Self::Bytes;
            }
            return Self::Vec(Box::new(parsed_inner));
        }
        if let Some(inner) = trimmed
            .strip_prefix("Secret<")
            .and_then(|s| s.strip_suffix('>'))
        {
            return Self::Secret(Box::new(Self::parse(inner)));
        }
        if let Some(inner) = trimmed
            .strip_prefix("HashMap<")
            .or_else(|| trimmed.strip_prefix("std::collections::HashMap<"))
            .and_then(|s| s.strip_suffix('>'))
        {
            if let Some((k, v)) = inner.split_once(',') {
                return Self::HashMap(Box::new(Self::parse(k)), Box::new(Self::parse(v)));
            }
        }
        if let Some(inner) = trimmed
            .strip_prefix("BTreeMap<")
            .or_else(|| trimmed.strip_prefix("std::collections::BTreeMap<"))
            .and_then(|s| s.strip_suffix('>'))
        {
            if let Some((k, v)) = inner.split_once(',') {
                return Self::BTreeMap(Box::new(Self::parse(k)), Box::new(Self::parse(v)));
            }
        }
        if let Some(inner) = trimmed.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            if inner.is_empty() {
                return Self::Tuple(vec![]);
            }
            let items = inner.split(',').map(Self::parse).collect();
            return Self::Tuple(items);
        }

        let last_seg = trimmed.rsplit("::").next().unwrap_or(trimmed);
        match last_seg {
            "bool" => Self::Primitive(PrimitiveType::Bool),
            "i8" => Self::Primitive(PrimitiveType::I8),
            "i16" => Self::Primitive(PrimitiveType::I16),
            "i32" => Self::Primitive(PrimitiveType::I32),
            "i64" => Self::Primitive(PrimitiveType::I64),
            "i128" => Self::Primitive(PrimitiveType::I128),
            "isize" => Self::Primitive(PrimitiveType::Isize),
            "u8" => Self::Primitive(PrimitiveType::U8),
            "u16" => Self::Primitive(PrimitiveType::U16),
            "u32" => Self::Primitive(PrimitiveType::U32),
            "u64" => Self::Primitive(PrimitiveType::U64),
            "u128" => Self::Primitive(PrimitiveType::U128),
            "usize" => Self::Primitive(PrimitiveType::Usize),
            "f32" => Self::Primitive(PrimitiveType::F32),
            "f64" => Self::Primitive(PrimitiveType::F64),
            "String" | "str" | "char" => Self::String,
            "Uuid" => Self::Uuid,
            "DateTime" | "DateTime<Utc>" => Self::DateTime,
            "NaiveDate" => Self::NaiveDate,
            "NaiveDateTime" => Self::NaiveDateTime,
            "Bytes" | "ByteBuf" => Self::Bytes,
            "Value" => Self::Value,
            other => Self::Custom(RustIdent::sanitize_pascal(other, "CustomType")),
        }
    }

    /// Unwraps `Option<T>` if present and returns the inner [`RustType`].
    pub fn unwrap_option(&self) -> &Self {
        match self {
            Self::Option(inner) => inner.unwrap_option(),
            other => other,
        }
    }

    /// Checks whether the outer type is `Option<T>`.
    pub fn is_option(&self) -> bool {
        matches!(self, Self::Option(_))
    }

    /// Checks whether this type (or its inner `Option<T>`) represents binary bytes (`Vec<u8>`, `Bytes`, `&[u8]`).
    pub fn is_bytes(&self) -> bool {
        matches!(self.unwrap_option(), Self::Bytes)
    }

    /// Checks whether this type is a built-in primitive or well-known standard library / crate type.
    pub fn is_primitive_or_known(&self) -> bool {
        match self {
            Self::Primitive(_)
            | Self::String
            | Self::Uuid
            | Self::DateTime
            | Self::NaiveDate
            | Self::NaiveDateTime
            | Self::Bytes
            | Self::Value => true,
            Self::Option(inner) | Self::Vec(inner) | Self::Secret(inner) => {
                inner.is_primitive_or_known()
            }
            Self::HashMap(k, v) | Self::BTreeMap(k, v) => {
                k.is_primitive_or_known() && v.is_primitive_or_known()
            }
            Self::Tuple(items) => items.iter().all(Self::is_primitive_or_known),
            Self::Custom(_) => false,
        }
    }

    /// Checks whether this type (or its inner `Option<T>`) is `Uuid`.
    pub fn is_uuid(&self) -> bool {
        matches!(self.unwrap_option(), Self::Uuid)
    }

    /// Checks whether this type (or its inner `Option<T>`) is `bool`.
    pub fn is_bool(&self) -> bool {
        matches!(self.unwrap_option(), Self::Primitive(PrimitiveType::Bool))
    }

    /// Checks whether this type (or its inner `Option<T>`) is an integer primitive.
    pub fn is_integer(&self) -> bool {
        matches!(
            self.unwrap_option(),
            Self::Primitive(
                PrimitiveType::I8
                    | PrimitiveType::I16
                    | PrimitiveType::I32
                    | PrimitiveType::I64
                    | PrimitiveType::I128
                    | PrimitiveType::Isize
                    | PrimitiveType::U8
                    | PrimitiveType::U16
                    | PrimitiveType::U32
                    | PrimitiveType::U64
                    | PrimitiveType::U128
                    | PrimitiveType::Usize
            )
        )
    }

    /// Returns a sample URL path segment string literal for unit testing this type.
    pub fn sample_url_segment(&self) -> &'static str {
        match self.unwrap_option() {
            Self::Uuid => "00000000-0000-0000-0000-000000000000",
            Self::Primitive(PrimitiveType::Bool) => "true",
            _ => "0",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ContentMediaType, CustomHttpMethod, ExtensionKey, HttpMethod, LinkParamValue, MediaType,
        OperationId, PrimitiveType, RuntimeExpression, RustIdent, RustType, StatusCodeSpec,
        StatusRange,
    };
    use serde_json::json;

    #[test]
    fn test_domain_types_http_method_status_media_ident_ext_rust_type() {
        assert_eq!("get".parse::<HttpMethod>().expect("get"), HttpMethod::Get);
        assert_eq!(
            "POST".parse::<HttpMethod>().expect("post"),
            HttpMethod::Post
        );
        assert_eq!("PUT".parse::<HttpMethod>().expect("put"), HttpMethod::Put);
        assert_eq!(
            "DELETE".parse::<HttpMethod>().expect("del"),
            HttpMethod::Delete
        );
        assert_eq!(
            "PATCH".parse::<HttpMethod>().expect("patch"),
            HttpMethod::Patch
        );
        assert_eq!(
            "OPTIONS".parse::<HttpMethod>().expect("opt"),
            HttpMethod::Options
        );
        assert_eq!(
            "HEAD".parse::<HttpMethod>().expect("head"),
            HttpMethod::Head
        );
        assert_eq!(
            "TRACE".parse::<HttpMethod>().expect("trace"),
            HttpMethod::Trace
        );
        assert_eq!(
            "QUERY".parse::<HttpMethod>().expect("query"),
            HttpMethod::Query
        );
        let custom = "PROPFIND".parse::<HttpMethod>().expect("custom");
        assert_eq!(custom.as_str(), "PROPFIND");
        assert_eq!(custom.to_lowercase_str(), "propfind");
        assert_eq!(format!("{}", custom), "PROPFIND");
        assert!(CustomHttpMethod::parse("bad method").is_err());
        assert!(CustomHttpMethod::parse("").is_err());

        assert_eq!(
            "200".parse::<StatusCodeSpec>().expect("200"),
            StatusCodeSpec::Code(200)
        );
        assert_eq!(
            "200"
                .parse::<StatusCodeSpec>()
                .expect("200")
                .representative_code(),
            200
        );
        assert!("099".parse::<StatusCodeSpec>().is_err());
        assert_eq!(
            "2XX".parse::<StatusCodeSpec>().expect("2XX"),
            StatusCodeSpec::Range(StatusRange::Success2xx)
        );
        assert_eq!(
            "1XX"
                .parse::<StatusCodeSpec>()
                .expect("1XX")
                .representative_code(),
            100
        );
        assert_eq!(
            "3XX"
                .parse::<StatusCodeSpec>()
                .expect("3XX")
                .representative_code(),
            302
        );
        assert_eq!(
            "4XX"
                .parse::<StatusCodeSpec>()
                .expect("4XX")
                .representative_code(),
            400
        );
        assert_eq!(
            "5XX"
                .parse::<StatusCodeSpec>()
                .expect("5XX")
                .representative_code(),
            500
        );
        assert_eq!(
            "default".parse::<StatusCodeSpec>().expect("default"),
            StatusCodeSpec::Default
        );
        assert_eq!(format!("{}", StatusCodeSpec::Code(201)), "201");
        assert_eq!(
            format!("{}", StatusCodeSpec::Range(StatusRange::Info1xx)),
            "1XX"
        );
        assert_eq!(
            format!("{}", StatusCodeSpec::Range(StatusRange::Success2xx)),
            "2XX"
        );
        assert_eq!(
            format!("{}", StatusCodeSpec::Range(StatusRange::Redirect3xx)),
            "3XX"
        );
        assert_eq!(
            format!("{}", StatusCodeSpec::Range(StatusRange::ClientError4xx)),
            "4XX"
        );
        assert_eq!(
            format!("{}", StatusCodeSpec::Range(StatusRange::ServerError5xx)),
            "5XX"
        );
        assert_eq!(format!("{}", StatusCodeSpec::Default), "default");
        assert!("999".parse::<StatusCodeSpec>().is_err());

        let mt = MediaType::parse("Application/Problem+JSON; charset=\"utf-8\"");
        assert_eq!(mt.top_type, "application");
        assert_eq!(mt.subtype, "problem+json");
        assert_eq!(mt.suffix.as_deref(), Some("json"));
        assert_eq!(
            mt.parameters.get("charset").map(String::as_str),
            Some("utf-8")
        );
        assert!(mt.is_json());
        assert!(!mt.is_sequential_json());
        assert!(MediaType::parse("application/jsonl").is_sequential_json());
        assert!(MediaType::parse("text/plain").is_text());
        assert!(MediaType::parse("text/event-stream").is_event_stream());
        assert!(MediaType::parse("application/x-www-form-urlencoded").is_form_urlencoded());
        assert!(MediaType::parse("multipart/form-data").is_multipart());
        assert_eq!(
            MediaType::normalize("Text/Plain; charset=utf-8"),
            "text/plain"
        );

        assert_eq!(RustIdent::sanitize_snake("type", "param").as_str(), "type_");
        assert_eq!(
            RustIdent::sanitize_snake("123-abc", "param").as_str(),
            "_123_abc"
        );
        assert_eq!(
            RustIdent::sanitize_pascal("my_model", "Model").as_str(),
            "MyModel"
        );
        assert_eq!(
            RustIdent::sanitize_pascal("123", "Schema").into_inner(),
            "Schema123"
        );
        assert_eq!(format!("{}", RustIdent::sanitize_snake("id", "p")), "id");

        let op_id = OperationId::parse("getUserById").expect("op_id");
        assert_eq!(op_id.as_str(), "getUserById");
        assert_eq!(op_id.to_handler_ident().as_str(), "getuserbyid");
        assert!(OperationId::parse("   ").is_err());

        let ext = ExtensionKey::parse("x-rate-limit").expect("ext");
        assert_eq!(ext.as_str(), "x-rate-limit");
        assert!(ExtensionKey::parse("rate-limit").is_err());
        assert!(ExtensionKey::parse("x-").is_err());
        let deser_ext: ExtensionKey = serde_json::from_str("\"x-custom\"").expect("deser ext");
        assert_eq!(deser_ext.as_str(), "x-custom");
        assert!(serde_json::from_str::<ExtensionKey>("\"invalid\"").is_err());

        assert_eq!(
            RustType::parse("Option<Uuid>").sample_url_segment(),
            "00000000-0000-0000-0000-000000000000"
        );
        assert_eq!(RustType::parse("bool").sample_url_segment(), "true");
        assert_eq!(RustType::parse("i64").sample_url_segment(), "0");
        assert!(RustType::parse("Option<Uuid>").is_uuid());
        assert!(RustType::parse("bool").is_bool());
        assert!(RustType::parse("u32").is_integer());
        assert_eq!(RustType::parse("Vec<u8>"), RustType::Bytes);
        assert_eq!(RustType::parse("&[u8]"), RustType::Bytes);
        assert_eq!(
            RustType::parse("Secret<String>"),
            RustType::Secret(Box::new(RustType::String))
        );
        assert_eq!(
            RustType::parse("std::collections::HashMap<String, i32>"),
            RustType::HashMap(
                Box::new(RustType::String),
                Box::new(RustType::Primitive(PrimitiveType::I32))
            )
        );
        assert_eq!(
            RustType::parse("std::collections::BTreeMap<String, serde_json::Value>"),
            RustType::BTreeMap(Box::new(RustType::String), Box::new(RustType::Value))
        );
        assert_eq!(
            RustType::parse("Vec<String>"),
            RustType::Vec(Box::new(RustType::String))
        );
        assert_eq!(
            RustType::parse("HashMap<String, i32>"),
            RustType::HashMap(
                Box::new(RustType::String),
                Box::new(RustType::Primitive(PrimitiveType::I32))
            )
        );
        assert_eq!(
            RustType::parse("BTreeMap<String, serde_json::Value>"),
            RustType::BTreeMap(Box::new(RustType::String), Box::new(RustType::Value))
        );
        assert!(RustType::parse("HashMap<String, i32>").is_primitive_or_known());
        assert!(!RustType::parse("HashMap<String, CustomVal>").is_primitive_or_known());
        assert!(!RustType::parse("HashMap<CustomKey, i32>").is_primitive_or_known());
        assert!(RustType::parse("Option<Vec<Secret<Uuid>>>").is_primitive_or_known());
        assert!(RustType::parse("(String, i32)").is_primitive_or_known());
        assert!(!RustType::parse("(String, CustomItem)").is_primitive_or_known());
        assert_eq!(RustType::parse("()"), RustType::Tuple(vec![]));
        assert_eq!(
            RustType::parse("(i8, i16, i128, isize, u16, u64, u128, usize, f32, f64, DateTime, NaiveDate, NaiveDateTime, Bytes, User)"),
            RustType::Tuple(vec![
                RustType::Primitive(PrimitiveType::I8),
                RustType::Primitive(PrimitiveType::I16),
                RustType::Primitive(PrimitiveType::I128),
                RustType::Primitive(PrimitiveType::Isize),
                RustType::Primitive(PrimitiveType::U16),
                RustType::Primitive(PrimitiveType::U64),
                RustType::Primitive(PrimitiveType::U128),
                RustType::Primitive(PrimitiveType::Usize),
                RustType::Primitive(PrimitiveType::F32),
                RustType::Primitive(PrimitiveType::F64),
                RustType::DateTime,
                RustType::NaiveDate,
                RustType::NaiveDateTime,
                RustType::Bytes,
                RustType::Custom(RustIdent::sanitize_pascal("User", "CustomType")),
            ])
        );
    }

    #[test]
    fn test_runtime_expression_helpers() {
        let expr = RuntimeExpression::new("$request.body#/id");
        assert_eq!(expr.as_str(), "$request.body#/id");
        assert!(expr.is_expression());

        let plain = RuntimeExpression::new("id");
        assert!(!plain.is_expression());
    }

    #[test]
    fn test_runtime_expression_brace_normalization() {
        let expr = RuntimeExpression::new("{$request.body#/id}");
        assert_eq!(expr.as_str(), "$request.body#/id");
        assert!(expr.is_expression());
    }

    #[test]
    fn test_runtime_expression_parse_validation() {
        let expr = RuntimeExpression::parse_expression("$request.path.id")
            .expect("Failed to parse expression");
        assert_eq!(expr.as_str(), "$request.path.id");

        let invalid = RuntimeExpression::parse_expression("$request.body#bad");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_runtime_expression_template_parse() {
        let expr = RuntimeExpression::parse_expression(
            "http://example.com?foo={$request.path.id}&bar={$response.header.Location}",
        )
        .expect("Failed to parse expression");
        assert!(expr.is_expression());

        let plain = RuntimeExpression::parse("no expressions here").expect("Failed to parse");
        assert!(!plain.is_expression());
    }

    #[test]
    fn test_link_param_value_literal() {
        let literal = LinkParamValue::Literal(json!(42));
        assert!(matches!(literal, LinkParamValue::Literal(_)));
    }

    #[test]
    fn test_runtime_expression_display_and_debug() {
        let expr = RuntimeExpression::new("$response.body#/name");
        let display = format!("{}", expr);
        assert_eq!(display, "$response.body#/name");

        let debug = format!("{:?}", expr);
        assert!(debug.contains("RuntimeExpression"));
        assert!(debug.contains("$response.body#/name"));
    }

    #[test]
    fn test_content_media_type_normalization() {
        assert_eq!(
            ContentMediaType::from_media_type("application/x-www-form-urlencoded"),
            ContentMediaType::FormUrlEncoded
        );
        assert_eq!(
            ContentMediaType::from_media_type("application/vnd.api+json; charset=utf-8"),
            ContentMediaType::Json
        );

        assert_eq!(
            ContentMediaType::from_media_type("text/plain"),
            ContentMediaType::Other("text/plain".to_string())
        );
        assert_eq!(
            ContentMediaType::FormUrlEncoded.as_str(),
            "application/x-www-form-urlencoded"
        );
        assert_eq!(ContentMediaType::Json.as_str(), "application/json");
        assert_eq!(
            ContentMediaType::Other("text/plain".to_string()).as_str(),
            "text/plain"
        );
        assert_eq!(
            ContentMediaType::from_media_type("application/*+json"),
            ContentMediaType::Json
        );

        // Cover OAuthFlows::is_empty, SecurityRequirementGroup::anonymous, ExampleValue::is_external
        let empty_flows = super::OAuthFlows {
            implicit: None,
            password: None,
            client_credentials: None,
            authorization_code: None,
            device_authorization: None,
        };
        assert!(empty_flows.is_empty());
        let sample_flow = super::OAuthFlow {
            authorization_url: None,
            device_authorization_url: None,
            token_url: None,
            refresh_url: None,
            scopes: std::collections::BTreeMap::new(),
        };
        for flows in [
            super::OAuthFlows {
                implicit: Some(sample_flow.clone()),
                ..empty_flows.clone()
            },
            super::OAuthFlows {
                password: Some(sample_flow.clone()),
                ..empty_flows.clone()
            },
            super::OAuthFlows {
                client_credentials: Some(sample_flow.clone()),
                ..empty_flows.clone()
            },
            super::OAuthFlows {
                authorization_code: Some(sample_flow.clone()),
                ..empty_flows.clone()
            },
            super::OAuthFlows {
                device_authorization: Some(sample_flow),
                ..empty_flows
            },
        ] {
            assert!(!flows.is_empty());
        }

        for m in [
            "GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD", "TRACE", "QUERY",
        ] {
            let parsed = m.parse::<HttpMethod>().expect("method");
            assert_eq!(parsed.as_str(), m);
        }

        for (s, code) in [
            ("1XX", 100),
            ("2XX", 200),
            ("3XX", 302),
            ("4XX", 400),
            ("5XX", 500),
            ("default", 200),
        ] {
            let spec = s.parse::<StatusCodeSpec>().expect("spec");
            assert_eq!(spec.representative_code(), code);
            assert_eq!(spec.to_string(), s);
        }

        let anon = super::SecurityRequirementGroup::anonymous();
        assert!(anon.is_anonymous());

        let ext_ex = super::ExampleValue::external(json!("http://example.com"));
        assert!(ext_ex.is_external());
        assert!(ext_ex.is_serialized());
        let data_ex = super::ExampleValue::data(json!(1)).with_overrides(None, None);
        assert!(!data_ex.is_external());

        // Cover RuntimeExpression::parse and parse_expression branches
        for valid_expr in [
            "$url",
            "$method",
            "$statusCode",
            "$self",
            "$request.header.X-Custom!#$%&'*+-.^_`|~0",
            "$request.query.q",
            "$request.path.p",
            "$request.body",
            "$request.body#",
            "$request.body#/a~0b/c~1d",
            "$response.body#/ok",
            "$message.header.X-Msg",
            "$message.header#/meta/id",
            "$message.payload#/data/id",
            "$inputs.userId",
            "$inputs.user#/id",
            "$outputs.resultId",
            "$steps.login.outputs.token",
            "$workflows.auth.outputs.jwt",
            "$sourceDescriptions.petstore.url",
            "$components.parameters.petId",
            "$components.successActions.done",
            "$components.failureActions.retry",
            "$components.inputs.UserInput",
            "prefix-{not_expr}-{unclosed-{$url}-suffix",
            "{$url}",
            "{$url}{unclosed",
            "{plain}",
        ] {
            assert!(RuntimeExpression::parse(valid_expr).is_ok());
        }
        assert!(RuntimeExpression::parse_expression("no_expr_at_all").is_err());
        assert!(RuntimeExpression::parse("bad-template-{$invalid}").is_err());

        for invalid_expr in [
            "$unknown",
            "$request.unknown",
            "$request.header.",
            "$request.header.bad header",
            "$request.query.",
            "$request.path.",
            "$request.body_invalid",
            "$request.body#no_slash",
            "$request.body#/~2",
            "$request.body#/~",
            "$message.unknown",
            "$message.header.",
            "$message.payload#no_slash",
            "$inputs.",
            "$inputs.a..b",
            "$inputs.a#no_slash",
            "$steps.only_id",
            "$steps..outputs.x",
            "$steps.s1.",
            "$components.only_section",
            "$components.unknown.x",
            "$components.parameters.",
            "$components.parameters.a.b",
        ] {
            assert!(RuntimeExpression::parse(invalid_expr).is_err());
            assert!(RuntimeExpression::parse_expression(invalid_expr).is_err());
        }

        // Cover StatusCodeSpec edge cases
        assert!("99".parse::<StatusCodeSpec>().is_err());
        assert!("abc".parse::<StatusCodeSpec>().is_err());

        // Cover MediaType edge cases
        let mt_no_slash = MediaType::parse("application; flag");
        assert_eq!(mt_no_slash.top_type, "application");
        assert_eq!(mt_no_slash.subtype, "*");
        assert!(MediaType::parse("application/x-ndjson").is_sequential_json());
        assert!(MediaType::parse("application/json-seq").is_sequential_json());
        assert!(MediaType::parse("application/geo+json-seq").is_sequential_json());
        assert!(MediaType::parse("application/vnd.custom+jsonl").is_sequential_json());
        assert!(MediaType::parse("application/vnd.custom+ndjson").is_sequential_json());
        assert!(MediaType::parse("application/vnd.custom+json-seq").is_sequential_json());
        assert!(MediaType::parse("application/*+json").is_json());
        assert!(MediaType::parse("application/xml").is_text());
        assert!(MediaType::parse("application/atom+xml").is_text());
        assert!(!MediaType::parse("application/octet-stream").is_text());
        assert!(!MediaType::parse("text/plain").is_event_stream());
        assert!(!MediaType::parse("application/json").is_form_urlencoded());

        // Cover RustIdent and RustType edge cases
        assert_eq!(RustIdent::sanitize_snake("---", "fb").as_str(), "fb");
        assert_eq!(RustIdent::sanitize_snake("a__b__", "fb").as_str(), "a_b");
        assert_eq!(RustIdent::sanitize_pascal("---", "Fb").as_str(), "Fb");
        assert_eq!(
            RustIdent::sanitize_pascal("Self", "Type").as_str(),
            "SelfType"
        );
        assert_eq!(
            RustType::parse("HashMap<Unclosed>"),
            RustType::Custom(RustIdent::sanitize_pascal(
                "HashMap<Unclosed>",
                "CustomType"
            ))
        );
        assert_eq!(
            RustType::parse("BTreeMap<Unclosed>"),
            RustType::Custom(RustIdent::sanitize_pascal(
                "BTreeMap<Unclosed>",
                "CustomType"
            ))
        );
        assert!(RustType::parse("Option<Vec<u8>>").is_option());
        assert!(RustType::parse("Option<Vec<u8>>").is_bytes());
        assert!(RustType::parse("Option<Vec<String>>").is_primitive_or_known());
        assert!(RustType::parse("HashMap<String, (i32, Uuid)>").is_primitive_or_known());
        assert!(!RustType::parse("Vec<CustomDto>").is_primitive_or_known());

        let mut ext_map = std::collections::BTreeMap::new();
        ext_map.insert("x-custom".to_string(), json!(true));
        let param = super::RouteParam {
            name: "id".to_string(),
            description: None,
            source: super::ParamSource::Path,
            ty: "Uuid".to_string(),
            content_media_type: None,
            style: None,
            explode: false,
            deprecated: false,
            allow_empty_value: false,
            allow_reserved: false,
            example: None,
            raw_schema: None,
            extensions: ext_map.clone(),
        };
        assert!(param.rust_type().is_uuid());
        assert_eq!(param.typed_extensions().len(), 1);

        let hdr = super::ResponseHeader {
            name: "X-Rate".to_string(),
            description: None,
            required: false,
            deprecated: false,
            style: None,
            explode: None,
            ty: "i32".to_string(),
            content_media_type: None,
            example: None,
            extensions: ext_map.clone(),
        };
        assert!(hdr.rust_type().is_integer());
        assert_eq!(hdr.typed_extensions().len(), 1);

        let rb = super::RequestBodyDefinition {
            ty: "Vec<u8>".to_string(),
            description: None,
            media_type: "application/json".to_string(),
            format: super::BodyFormat::Binary,
            required: true,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
            example: None,
        };
        assert!(rb.rust_type().is_bytes());
        assert!(rb.parsed_media_type().is_json());

        let route = super::ParsedRoute {
            path: "/pets".to_string(),
            summary: None,
            description: None,
            path_summary: None,
            path_description: None,
            operation_summary: None,
            operation_description: None,
            path_extensions: std::collections::BTreeMap::new(),
            base_path: None,
            path_servers: None,
            servers_override: None,
            method: "POST".to_string(),
            handler_name: "create_pet".to_string(),
            operation_id: Some("createPet".to_string()),
            params: vec![param],
            path_params: vec![],
            request_body: Some(rb),
            raw_request_body: None,
            security: vec![],
            security_defined: false,
            kind: super::RouteKind::Path,
            tags: vec![],
            response_type: Some("String".to_string()),
            response_status: Some("201".to_string()),
            response_summary: None,
            response_description: None,
            response_media_type: Some("application/json".to_string()),
            response_example: None,
            response_headers: vec![hdr],
            raw_responses: None,
            response_links: None,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            extensions: ext_map,
        };
        assert_eq!(route.http_method(), HttpMethod::Post);
        assert_eq!(route.handler_ident().as_str(), "create_pet");
        assert_eq!(
            route.typed_operation_id().expect("op id").as_str(),
            "createPet"
        );
        assert_eq!(
            route.response_status_spec(),
            Some(StatusCodeSpec::Code(201))
        );
        assert!(route.response_media_type_parsed().expect("mt").is_json());
        assert_eq!(route.response_rust_type(), Some(RustType::String));
        assert_eq!(route.typed_extensions().len(), 1);
    }
}
