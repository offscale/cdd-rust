#![deny(missing_docs)]

//! # OpenAPI Models
//!
//! definitions for Intermediate Representation of OpenAPI elements.
//!
//! These structs are used to transport parsed data from the YAML spec
//! into the code generation strategies.

/// Distinguishes between standard paths and event-driven webhooks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteKind {
    /// A standard HTTP endpoint defined in `paths`.
    Path,
    /// An event receiver defined in `webhooks`.
    Webhook,
}

/// Represents a parsed API route.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedRoute {
    /// The URL path (e.g. "/users/{id}") or Webhook Name (e.g. "userCreated").
    pub path: String,
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
    /// The Rust type name of the success response (e.g. `UserResponse`, `Vec<User>`).
    /// Only present if a 200/201 response with application/json content is defined inline.
    pub response_type: Option<String>,
    /// The classification of this route.
    pub kind: RouteKind,
    /// Callback definitions attached to this route (OAS 3.0+).
    pub callbacks: Vec<ParsedCallback>,
}

/// Represents a callback definition (outgoing webhook).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCallback {
    /// The callback name (key in the callbacks map).
    pub name: String,
    /// The runtime expression URL (e.g. "{$request.query.callbackUrl}").
    pub expression: String,
    /// The HTTP method for the callback request.
    pub method: String,
    /// The expected body sent in the callback (if any).
    pub request_body: Option<RequestBodyDefinition>,
    /// The expected response type from the callback receiver.
    pub response_type: Option<String>,
}

/// A single security requirement (AND logic).
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityRequirement {
    /// Name of the security scheme in components.
    pub scheme_name: String,
    /// Required scopes (for OAuth2/OIDC).
    pub scopes: Vec<String>,
}

/// Definition of a request body type and format.
#[derive(Debug, Clone, PartialEq)]
pub struct RequestBodyDefinition {
    /// The Rust type name (e.g. "CreateUserRequest")
    pub ty: String,
    /// The format of the body (JSON, Form, etc.)
    pub format: BodyFormat,
}

/// Supported body content types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyFormat {
    /// application/json
    Json,
    /// application/x-www-form-urlencoded
    Form,
    /// multipart/form-data
    Multipart,
}

/// Parameter serialization style as defined in RFC 6570 and OpenAPI 3.x.
#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Default)]
pub enum ParamStyle {
    /// Path-style parameters defined by RFC6570 (matrix).
    Matrix,
    /// Label style parameters defined by RFC6570 (label).
    Label,
    /// Form style parameters defined by RFC6570 (form).
    /// Standard for query (e.g. `id=5&name=foo`) and cookie.
    Form,
    /// Simple style parameters defined by RFC6570 (simple).
    /// Standard for path (e.g. `/users/5,6,7`) and header.
    #[default]
    Simple,
    /// Space separated array values (e.g. `key=a b c`).
    SpaceDelimited,
    /// Pipe separated array values (e.g. `key=a|b|c`).
    PipeDelimited,
    /// Deep object serialization (e.g. `key[prop]=val`).
    DeepObject,
}

/// Represents a parameter in a route (Path, Query, Header, Cookie).
#[derive(Debug, Clone, PartialEq)]
pub struct RouteParam {
    /// Parameter name in the source (e.g. "id")
    pub name: String,
    /// Whether it's from Path, Query, Header, or Cookie.
    pub source: ParamSource,
    /// Rust type (e.g. `Uuid`, `i32`, `Option<String>`)
    pub ty: String,
    /// Serialization style (e.g. Form, Simple).
    pub style: Option<ParamStyle>,
    /// Whether arrays/objects generate separate parameters.
    pub explode: bool,
    /// Whether reserved characters (RFC3986) are allowed.
    pub allow_reserved: bool,
}

/// The source location of a parameter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParamSource {
    /// URL Path parameter (e.g. /users/{id})
    Path,
    /// URL Query parameter (e.g. /users?page=1)
    Query,
    /// HTTP Header parameter (e.g. X-Request-ID: 123)
    Header,
    /// HTTP Cookie parameter (e.g. SessionId=abc)
    Cookie,
}
