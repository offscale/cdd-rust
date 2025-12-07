#![deny(missing_docs)]

//! # Route Shims
//!
//! Generic structures acting as an Intermediate Deserialization Layer.
//! These structs map directly to OpenAPI YAML objects.
//!
//! Note: Top-level shims do not derive `Debug` because `utoipa::RefOr` and `utoipa::Responses`
//! do not implement `Debug`.

use crate::oas::resolver::ShimParameter;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use utoipa::openapi::request_body::RequestBody;
use utoipa::openapi::{RefOr, Responses};

/// Schema for the root document (Paths and Webhooks).
#[derive(Deserialize, Serialize)]
pub struct ShimOpenApi {
    /// OpenAPI version (e.g. "3.2.0").
    /// Required in OAS 3.x.
    pub openapi: Option<String>,

    /// Swagger version (e.g. "2.0") for legacy support.
    pub swagger: Option<String>,

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

    /// Path items.
    #[serde(default)]
    pub paths: BTreeMap<String, ShimPathItem>,

    /// Webhook items.
    #[serde(default)]
    pub webhooks: Option<BTreeMap<String, RefOr<ShimPathItem>>>,

    /// Global security requirements.
    #[serde(default)]
    pub security: Option<Vec<Value>>,

    /// Tags used by the specification with additional metadata.
    #[serde(default)]
    pub tags: Option<Vec<ShimTag>>,

    /// External documentation.
    #[serde(rename = "externalDocs")]
    pub external_docs: Option<ShimExternalDocs>,
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

    /// Capture other loosely typed component maps (schemas, parameters, etc.)
    /// to maintain compatibility with existing resolvers that expect generic Value lookups.
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
}

/// OpenID Connect definition.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ShimOpenIdConnect {
    /// Connect URL.
    #[serde(rename = "openIdConnectUrl")]
    pub open_id_connect_url: String,
    /// Description.
    pub description: Option<String>,
}

/// Mutual TLS definition.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ShimMutualTls {
    /// Description.
    pub description: Option<String>,
}

/// OAuth2 definition.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ShimOAuth2 {
    /// Description.
    pub description: Option<String>,
    /// Supported flows.
    pub flows: Option<ShimOAuthFlows>,
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
}

/// Single OAuth Flow definition.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ShimOAuthFlow {
    /// Authorization URL.
    #[serde(rename = "authorizationUrl")]
    pub authorization_url: Option<String>,
    /// Token URL.
    #[serde(rename = "tokenUrl")]
    pub token_url: Option<String>,
    /// Refresh URL.
    #[serde(rename = "refreshUrl")]
    pub refresh_url: Option<String>,
    /// Available scopes and descriptions.
    #[serde(default)]
    pub scopes: BTreeMap<String, String>,
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
}

/// An object representing a Server.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ShimServer {
    /// A URL to the target host.
    pub url: String,
    /// An optional string describing the host.
    pub description: Option<String>,
    /// A map between a variable name and its value.
    #[serde(default)]
    pub variables: Option<BTreeMap<String, ShimServerVariable>>,
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
}

/// Allows referencing an external resource for extended documentation.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ShimExternalDocs {
    /// A description of the target documentation.
    pub description: Option<String>,
    /// The URL for the target documentation.
    pub url: String,
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
}

/// A Path Item containing operations for a specific URL/Webhook.
#[derive(Deserialize, Serialize, Clone)]
pub struct ShimPathItem {
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
}

/// A single HTTP Operation definition.
#[derive(Deserialize, Serialize, Clone)]
pub struct ShimOperation {
    /// Unique identifier for the operation.
    #[serde(rename = "operationId")]
    pub operation_id: Option<String>,
    /// Operation-specific parameters.
    #[serde(default)]
    pub parameters: Option<Vec<RefOr<ShimParameter>>>,
    /// Request Body.
    #[serde(rename = "requestBody")]
    pub request_body: Option<RefOr<RequestBody>>,
    /// Responses.
    #[serde(default)]
    pub responses: Responses,
    /// Security requirements (raw JSON values to be generic).
    #[serde(default)]
    pub security: Option<Vec<Value>>,
    /// Callbacks definition.
    #[serde(default)]
    pub callbacks: Option<BTreeMap<String, RefOr<BTreeMap<String, ShimPathItem>>>>,
    /// A list of tags for API documentation control.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
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
}
