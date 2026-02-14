#![deny(missing_docs)]

//! # OpenAPI Models
//!
//! definition of Intermediate Representation (IR) structures for parsed OpenAPI elements.
//!
//! These structs are used to transport parsed data from the YAML spec
//! into the code generation strategies.

use crate::error::{AppError, AppResult};
use crate::parser::models::ParsedExternalDocs;
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

/// Represents a validated Runtime Expression or template (OAS 3.2 ABNF).
///
/// Syntax: `$url` | `$method` | `$statusCode` | `$request.{source}` | `$response.{source}`
/// Templates may embed expressions inside `{}` within larger strings.
#[derive(Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct RuntimeExpression(String);

impl RuntimeExpression {
    /// Creates a new RuntimeExpression from a string.
    /// Note: Does not currently enforce strict ABNF validation on creation,
    /// but is typed to distinguish from standard strings.
    pub fn new(s: impl Into<String>) -> Self {
        let raw = s.into();
        let normalized = normalize_runtime_expression(&raw);
        Self(normalized)
    }

    /// Parses and validates a runtime expression or template, allowing constants.
    ///
    /// If the value is not a runtime expression (does not start with `$` and
    /// contains no `{...}` runtime segments), the value is accepted as a literal.
    pub fn parse(s: impl AsRef<str>) -> AppResult<Self> {
        let raw = s.as_ref();
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
    pub fn parse_expression(s: impl AsRef<str>) -> AppResult<Self> {
        let raw = s.as_ref();
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
        flows: OAuthFlows,
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

#[cfg(test)]
mod tests {
    use super::{ContentMediaType, LinkParamValue, RuntimeExpression};
    use serde_json::json;

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
        let expr = RuntimeExpression::parse_expression("$request.path.id").unwrap();
        assert_eq!(expr.as_str(), "$request.path.id");

        let invalid = RuntimeExpression::parse_expression("$request.body#bad");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_runtime_expression_template_parse() {
        let expr = RuntimeExpression::parse_expression(
            "http://example.com?foo={$request.path.id}&bar={$response.header.Location}",
        )
        .unwrap();
        assert!(expr.is_expression());

        let plain = RuntimeExpression::parse("no expressions here").unwrap();
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

        match ContentMediaType::from_media_type("text/plain") {
            ContentMediaType::Other(val) => assert_eq!(val, "text/plain"),
            _ => panic!("Expected Other for text/plain"),
        }
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
}

fn normalize_runtime_expression(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') && trimmed.len() >= 2 {
        let inner = trimmed[1..trimmed.len() - 1].trim();
        if inner.starts_with('$') {
            return inner.to_string();
        }
    }

    trimmed.to_string()
}

/// A parsed segment of a runtime expression template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeExpressionSegment {
    /// Literal text segment.
    Literal(String),
    /// Embedded runtime expression (without surrounding braces).
    Expression(String),
}

/// Splits a runtime expression template into literal and expression segments.
///
/// Expressions are identified as `{...}` segments whose trimmed inner text starts with `$`.
/// Non-expression braces are preserved as literals.
pub(crate) fn split_runtime_expression_template(raw: &str) -> Vec<RuntimeExpressionSegment> {
    let mut segments = Vec::new();
    let mut buf = String::new();
    let mut chars = raw.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            let mut inner = String::new();
            let mut found_end = false;
            while let Some(n) = chars.next() {
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

fn contains_runtime_expression(raw: &str) -> bool {
    split_runtime_expression_template(raw)
        .iter()
        .any(|seg| matches!(seg, RuntimeExpressionSegment::Expression(_)))
}

fn validate_runtime_expression(expr: &str) -> AppResult<()> {
    match expr {
        "$url" | "$method" | "$statusCode" => return Ok(()),
        _ => {}
    }

    if let Some(rest) = expr.strip_prefix("$request.") {
        return validate_runtime_source(rest);
    }

    if let Some(rest) = expr.strip_prefix("$response.") {
        return validate_runtime_source(rest);
    }

    Err(AppError::General(format!(
        "Invalid runtime expression: '{}'",
        expr
    )))
}

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
