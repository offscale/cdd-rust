#![deny(missing_docs)]

//! # OpenAPI Document Parsing
//!
//! Provides a higher-level parser that extracts both routes and top-level
//! OpenAPI document metadata for round-trip workflows.

use crate::error::{AppError, AppResult};
use crate::oas::models::ParsedRoute;
use crate::oas::registry::DocumentRegistry;
use crate::oas::routes::builder::parse_security_requirements;
use crate::oas::routes::shims::{ShimOpenApi, ShimServer, ShimServerVariable, ShimTag};
use crate::oas::schemas::refs::compute_base_uri;
use crate::oas::validation::validate_openapi_root;
use crate::schema_generator::{
    OpenApiContact, OpenApiInfo, OpenApiLicense, OpenApiServer, OpenApiServerVariable, OpenApiTag,
};
use crate::{parse_openapi_routes, parse_openapi_routes_with_registry, parser::ParsedExternalDocs};
use std::collections::BTreeMap;
use url::Url;

/// Parsed OpenAPI document metadata plus route definitions.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedOpenApi {
    /// Document-level metadata mapped to OpenAPI generation structures.
    pub info: OpenApiInfo,
    /// Parsed route definitions (paths + webhooks).
    pub routes: Vec<ParsedRoute>,
    /// Optional JSON Schema dialect declared by the document.
    pub json_schema_dialect: Option<String>,
    /// Raw `components` object for round-trip preservation.
    pub components: Option<serde_json::Value>,
}

/// Parses an OpenAPI document into routes and top-level metadata for round-trip use.
///
/// This function keeps OpenAPI ➜ Rust workflows intact while preserving:
/// - `$self` (base URI)
/// - `jsonSchemaDialect`
/// - `info`
/// - `servers` (including variables)
/// - `tags` (including metadata)
/// - top-level `externalDocs`
/// - Paths/Webhooks `x-` extensions
pub fn parse_openapi_document(yaml_content: &str) -> AppResult<ParsedOpenApi> {
    parse_openapi_document_with_registry(yaml_content, None, None)
}

/// Parses an OpenAPI document into routes and metadata with optional external reference support.
pub fn parse_openapi_document_with_registry(
    yaml_content: &str,
    registry: Option<&DocumentRegistry>,
    retrieval_uri: Option<&str>,
) -> AppResult<ParsedOpenApi> {
    let raw_value: serde_json::Value = serde_yaml::from_str(yaml_content)
        .map_err(|e| AppError::General(format!("Failed to parse OpenAPI YAML: {}", e)))?;
    let shim: ShimOpenApi = serde_yaml::from_str(yaml_content)
        .map_err(|e| AppError::General(format!("Failed to parse OpenAPI YAML: {}", e)))?;
    validate_openapi_root(&shim)?;

    let routes = if registry.is_some() || retrieval_uri.is_some() {
        parse_openapi_routes_with_registry(yaml_content, registry, retrieval_uri)?
    } else {
        parse_openapi_routes(yaml_content)?
    };

    let mut info = build_openapi_info(&shim)?;
    if let Some(requirements) = shim.security.as_ref() {
        let base_uri = resolve_entry_base_uri(&shim, retrieval_uri);
        let mut owned_components = shim.components.clone();
        if let (Some(self_uri), Some(comps)) = (shim.self_uri.as_deref(), owned_components.as_mut())
        {
            comps.extra.insert(
                "__self".to_string(),
                serde_json::Value::String(self_uri.to_string()),
            );
        }
        info.security = parse_security_requirements(
            requirements,
            owned_components.as_ref().or(shim.components.as_ref()),
            registry,
            base_uri.as_ref(),
        );
    }

    Ok(ParsedOpenApi {
        info,
        routes,
        json_schema_dialect: shim.json_schema_dialect.clone(),
        components: raw_value.get("components").cloned(),
    })
}

fn build_openapi_info(shim: &ShimOpenApi) -> AppResult<OpenApiInfo> {
    let info = shim.info.as_ref().ok_or_else(|| {
        AppError::General("OpenAPI document missing required 'info' object".into())
    })?;

    let mut out = OpenApiInfo::new(info.title.clone(), info.version.clone());
    out.summary = info.summary.clone();
    out.description = info.description.clone();
    out.terms_of_service = info.terms_of_service.clone();
    out.self_uri = shim.self_uri.clone();
    out.external_docs = shim.external_docs.as_ref().map(|doc| ParsedExternalDocs {
        url: doc.url.clone(),
        description: doc.description.clone(),
    });

    out.contact = info.contact.as_ref().map(|c| OpenApiContact {
        name: c.name.clone(),
        url: c.url.clone(),
        email: c.email.clone(),
    });

    out.license = info.license.as_ref().map(|l| OpenApiLicense {
        name: l.name.clone(),
        identifier: l.identifier.clone(),
        url: l.url.clone(),
    });

    out.servers = shim
        .servers
        .as_ref()
        .map(|servers| servers.iter().map(map_server).collect())
        .unwrap_or_default();

    out.tags = shim
        .tags
        .as_ref()
        .map(|tags| tags.iter().map(map_tag).collect())
        .unwrap_or_default();

    out.paths_extensions = shim
        .paths
        .as_ref()
        .map(|paths| paths.extensions.clone())
        .unwrap_or_default();

    out.webhooks_extensions = shim
        .webhooks
        .as_ref()
        .map(|webhooks| webhooks.extensions.clone())
        .unwrap_or_default();

    out.extensions = filter_extensions(&shim.extensions);

    Ok(out)
}

fn resolve_entry_base_uri(openapi: &ShimOpenApi, retrieval_uri: Option<&str>) -> Option<Url> {
    let base_str = match (retrieval_uri, openapi.self_uri.as_deref()) {
        (Some(retrieval), Some(self_val)) => Some(compute_base_uri(retrieval, Some(self_val))),
        (Some(retrieval), None) => Some(retrieval.to_string()),
        (None, Some(self_val)) => Some(self_val.to_string()),
        (None, None) => None,
    }?;

    if let Ok(url) = Url::parse(&base_str) {
        return Some(url);
    }
    let dummy = Url::parse("http://example.invalid/").ok()?;
    if base_str.starts_with('/') {
        return dummy.join(&base_str).ok();
    }
    dummy.join(&base_str).ok()
}

fn map_server(server: &ShimServer) -> OpenApiServer {
    let mut out = OpenApiServer::new(server.url.clone());
    out.description = server.description.clone();
    out.name = server.name.clone();
    if let Some(vars) = &server.variables {
        out.variables = vars
            .iter()
            .map(|(k, v)| (k.clone(), map_server_var(v)))
            .collect();
    }
    out
}

fn map_server_var(var: &ShimServerVariable) -> OpenApiServerVariable {
    OpenApiServerVariable {
        enum_values: var.enum_values.clone(),
        default: var.default.clone(),
        description: var.description.clone(),
    }
}

fn map_tag(tag: &ShimTag) -> OpenApiTag {
    let mut out = OpenApiTag::new(tag.name.clone());
    out.summary = tag.summary.clone();
    out.description = tag.description.clone();
    out.parent = tag.parent.clone();
    out.kind = tag.kind.clone();
    out.external_docs = tag.external_docs.as_ref().map(|doc| ParsedExternalDocs {
        url: doc.url.clone(),
        description: doc.description.clone(),
    });
    out
}

fn filter_extensions(
    extensions: &BTreeMap<String, serde_json::Value>,
) -> BTreeMap<String, serde_json::Value> {
    extensions
        .iter()
        .filter(|(key, _)| key.starts_with("x-"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_openapi_document_preserves_metadata() {
        let yaml = r#"
openapi: 3.2.0
$self: https://example.com/openapi.yaml
jsonSchemaDialect: https://spec.openapis.org/oas/3.1/dialect/base
info:
  title: Example API
  version: 1.0.0
  summary: Short summary
  description: Longer description
  termsOfService: https://example.com/terms
  contact:
    name: API Support
    url: https://example.com/support
    email: support@example.com
  license:
    name: Apache 2.0
    identifier: Apache-2.0
components:
  securitySchemes:
    api_key:
      type: apiKey
      name: X-API-Key
      in: header
  examples:
    Sample:
      value: { id: 1 }
security:
  - api_key: []
externalDocs:
  url: https://example.com/docs
  description: Root docs
x-root: true
servers:
  - url: https://{tenant}.example.com/api
    name: prod
    description: Production server
    variables:
      tenant:
        default: acme
        enum: [acme, beta]
        description: Tenant id
tags:
  - name: accounts
    summary: Accounts
    description: Account APIs
    parent: external
    kind: nav
    externalDocs:
      url: https://example.com/accounts
      description: Account docs
  - name: external
paths:
  x-paths-meta:
    owner: api
  /users:
    get:
      responses:
        "200":
          description: ok
webhooks:
  x-webhooks-meta: true
  userCreated:
    post:
      responses:
        "200":
          description: ok
"#;

        let parsed = parse_openapi_document(yaml).unwrap();
        assert_eq!(parsed.info.title, "Example API");
        assert_eq!(parsed.info.version, "1.0.0");
        assert_eq!(parsed.info.summary.as_deref(), Some("Short summary"));
        assert_eq!(
            parsed.info.description.as_deref(),
            Some("Longer description")
        );
        assert_eq!(
            parsed.info.terms_of_service.as_deref(),
            Some("https://example.com/terms")
        );
        assert_eq!(
            parsed.info.self_uri.as_deref(),
            Some("https://example.com/openapi.yaml")
        );
        assert!(parsed
            .components
            .as_ref()
            .and_then(|c| c.get("examples"))
            .and_then(|e| e.get("Sample"))
            .is_some());
        assert_eq!(
            parsed.json_schema_dialect.as_deref(),
            Some("https://spec.openapis.org/oas/3.1/dialect/base")
        );
        assert_eq!(
            parsed.info.external_docs.as_ref().map(|d| d.url.as_str()),
            Some("https://example.com/docs")
        );
        assert_eq!(parsed.info.security.len(), 1);
        assert_eq!(parsed.info.security[0].schemes.len(), 1);
        assert_eq!(parsed.info.security[0].schemes[0].scheme_name, "api_key");
        assert_eq!(parsed.info.servers.len(), 1);
        assert_eq!(parsed.info.servers[0].name.as_deref(), Some("prod"));
        assert_eq!(
            parsed
                .info
                .paths_extensions
                .get("x-paths-meta")
                .and_then(|v| v.get("owner"))
                .and_then(|v| v.as_str()),
            Some("api")
        );
        assert_eq!(
            parsed
                .info
                .webhooks_extensions
                .get("x-webhooks-meta")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            parsed.info.servers[0]
                .variables
                .get("tenant")
                .map(|v| v.default.as_str()),
            Some("acme")
        );
        assert_eq!(parsed.info.tags.len(), 2);
        assert_eq!(
            parsed.info.extensions.get("x-root"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(parsed.info.tags[0].name, "accounts");
        assert_eq!(
            parsed.info.tags[0]
                .external_docs
                .as_ref()
                .map(|d| d.url.as_str()),
            Some("https://example.com/accounts")
        );
        assert_eq!(parsed.routes.len(), 2);
    }
}
