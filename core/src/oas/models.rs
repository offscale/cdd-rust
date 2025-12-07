#![deny(missing_docs)]

//! # OpenAPI Models
//!
//! definition of Intermediate Representation (IR) structures for parsed OpenAPI elements.
//!
//! These structs are used to transport parsed data from the YAML spec
//! into the code generation strategies.

use crate::parser::models::ParsedExternalDocs;
use std::collections::HashMap;
use std::fmt;

/// Distinguishes between standard paths and event-driven webhooks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteKind {
    /// A standard HTTP endpoint defined in `paths`.
    Path,
    /// An event receiver defined in `webhooks`.
    Webhook,
}

/// Represents a validated Runtime Expression (OAS 3.2 ABNF).
///
/// Syntax: `$url` | `$method` | `$statusCode` | `$request.{source}` | `$response.{source}`
#[derive(Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct RuntimeExpression(String);

impl RuntimeExpression {
    /// Creates a new RuntimeExpression from a string.
    /// Note: Does not currently enforce strict ABNF validation on creation,
    /// but is typed to distinguish from standard strings.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Returns the raw expression string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Heuristic check if the string looks like an expression (starts with `$`).
    pub fn is_expression(&self) -> bool {
        self.0.starts_with('$')
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
    /// The base URL path derived from the `servers` block (e.g. "/api/v1").
    pub base_path: Option<String>,
    /// HTTP Method: "GET", "POST", etc.
    pub method: String,
    /// Rust handler name, typically snake_case of operationId
    pub handler_name: String,
    /// Route parameters (path, query, header, cookie)
    pub params: Vec<RouteParam>,
    /// Request body definition (if any).
    pub request_body: Option<RequestBodyDefinition>,
    /// Security requirements.
    pub security: Vec<SecurityRequirement>,
    /// The classification of this route.
    pub kind: RouteKind,
    /// Tags associated with the operation (used for grouping/module organization).
    pub tags: Vec<String>,
    /// The Rust type name of the success response (e.g. `UserResponse`, `Vec<User>`).
    /// Only present if a 200/201 response with application/json content is defined inline.
    pub response_type: Option<String>,
    /// Response headers defined in the operation.
    pub response_headers: Vec<ResponseHeader>,
    /// Response links defined in the operation (HATEOAS).
    pub response_links: Option<Vec<ParsedLink>>,
    /// Callback definitions attached to this route (OAS 3.0+).
    pub callbacks: Vec<ParsedCallback>,
    /// Whether this route is deprecated.
    pub deprecated: bool,
    /// External documentation link.
    pub external_docs: Option<ParsedExternalDocs>,
}

/// Represents a header returned in the response.
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseHeader {
    /// Name of the header (e.g., "X-Rate-Limit-Limit").
    pub name: String,
    /// Description of the header.
    pub description: Option<String>,
    /// Rust type of the header value (e.g., "i32", "String").
    pub ty: String,
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
    /// Parameters to pass to the linked operation.
    /// Key: Parameter name, Value: Runtime expression.
    pub parameters: HashMap<String, RuntimeExpression>,
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
    /// The expected body sent in the callback (if any).
    pub request_body: Option<RequestBodyDefinition>,
    /// The expected response type from the callback receiver.
    pub response_type: Option<String>,
    /// Headers expected in the callback receiver's response.
    pub response_headers: Vec<ResponseHeader>,
}

/// Detailed information about a Security Scheme.
#[derive(Debug, Clone, PartialEq)]
pub struct SecuritySchemeInfo {
    /// The type of scheme (ApiKey, Http, etc.).
    pub kind: SecuritySchemeKind,
    /// The description provided in the spec.
    pub description: Option<String>,
}

/// Classification of the security scheme logic.
#[derive(Debug, Clone, PartialEq)]
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
    OAuth2,
    /// OpenID Connect.
    OpenIdConnect,
    /// Mutual TLS.
    MutualTls,
}

/// A single security requirement (AND logic).
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityRequirement {
    /// Name of the security scheme in components.
    pub scheme_name: String,
    /// Required scopes (for OAuth2/OIDC).
    pub scopes: Vec<String>,
    /// Resolved scheme details (if available).
    pub scheme: Option<SecuritySchemeInfo>,
}

/// Definition of a request body type and format.
#[derive(Debug, Clone, PartialEq)]
pub struct RequestBodyDefinition {
    /// The Rust type name (e.g. "CreateUserRequest").
    pub ty: String,
    /// The format of the body (JSON, Form, etc.).
    pub format: BodyFormat,
    /// Multipart/Form Encoding details.
    pub encoding: Option<HashMap<String, EncodingInfo>>,
}

/// Encoding details for a specific property.
#[derive(Debug, Clone, PartialEq)]
pub struct EncodingInfo {
    /// Content-Type.
    pub content_type: Option<String>,
    /// Headers map.
    pub headers: HashMap<String, String>,
}

/// Supported body content types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyFormat {
    /// application/json
    Json,
    /// application/x-www-form-urlencoded
    Form,
    /// multipart/form-data or multipart/mixed
    Multipart,
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
    /// Location.
    pub source: ParamSource,
    /// Rust type.
    pub ty: String,
    /// Serialization style.
    pub style: Option<ParamStyle>,
    /// Explode modifier.
    pub explode: bool,
    /// Allow reserved characters.
    pub allow_reserved: bool,
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
