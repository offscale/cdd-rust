#![deny(missing_docs)]

//! # Schema Generator
//!
//! Utilities for converting parsed Rust structs and enums into JSON Schema definitions.
//! This module enables the generation of OpenAPI-compliant schemas directly from
//! Rust source code models, respecting Serde attributes like `rename`, `rename_all`,
//! `deny_unknown_fields`, `skip`, `skip_serializing`, `skip_deserializing`, `tag`, and `untagged`.

use crate::classes::parse::{
    ParsedEnum, ParsedExternalDocs, ParsedField, ParsedModel, ParsedStruct, RenameRule,
};
use crate::error::AppError;
use crate::error::AppResult;
use crate::openapi::parse::models::{
    ExampleValue, LinkParamValue, LinkRequestBody, ParamSource, ParamStyle, ParsedLink,
    ParsedRoute, ParsedServer, ParsedServerVariable, RouteKind, RouteParam, SecuritySchemeInfo,
    SecuritySchemeKind,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// Minimal OpenAPI Info metadata for OpenAPI document generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiInfo {
    /// The title of the API.
    pub title: String,
    /// A short summary of the API.
    pub summary: Option<String>,
    /// The version of the API document.
    pub version: String,
    /// Optional description for the API.
    pub description: Option<String>,
    /// Optional Terms of Service URL.
    pub terms_of_service: Option<String>,
    /// Optional contact information.
    pub contact: Option<OpenApiContact>,
    /// Optional license information.
    pub license: Option<OpenApiLicense>,
    /// Optional `$self` URI for the OpenAPI document.
    ///
    /// When provided, the generated document will include a `$self` field
    /// establishing the base URI per OAS 3.2 Appendix F.
    pub self_uri: Option<String>,
    /// Optional external documentation for the OpenAPI document.
    pub external_docs: Option<ParsedExternalDocs>,
    /// Optional explicit servers to emit at the OpenAPI root.
    pub servers: Vec<OpenApiServer>,
    /// Optional tag metadata to emit at the OpenAPI root.
    pub tags: Vec<OpenApiTag>,
    /// Optional top-level security requirements to emit at the OpenAPI root.
    ///
    /// When provided, operations inherit these requirements unless overridden
    /// by their own `security` definitions.
    pub security: Vec<crate::openapi::parse::models::SecurityRequirementGroup>,
    /// Specification extensions (`x-...`) attached to the Paths Object.
    pub paths_extensions: BTreeMap<String, Value>,
    /// Specification extensions (`x-...`) attached to the Webhooks Object.
    pub webhooks_extensions: BTreeMap<String, Value>,
    /// Specification extensions (`x-...`) to emit at the OpenAPI root.
    pub extensions: BTreeMap<String, Value>,
}

impl OpenApiInfo {
    /// Creates a new OpenApiInfo with required fields.
    ///
    /// # Arguments
    /// * `title` - Input `title` parameter.
    /// * `version` - Input `version` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn new(title: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            summary: None,
            version: version.into(),
            description: None,
            terms_of_service: None,
            contact: None,
            license: None,
            self_uri: None,
            external_docs: None,
            servers: Vec::new(),
            tags: Vec::new(),
            security: Vec::new(),
            paths_extensions: BTreeMap::new(),
            webhooks_extensions: BTreeMap::new(),
            extensions: BTreeMap::new(),
        }
    }

    /// Sets an optional summary.
    ///
    /// # Arguments
    /// * `summary` - Input `summary` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Sets an optional description.
    ///
    /// # Arguments
    /// * `description` - Input `description` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the Terms of Service URL.
    ///
    /// # Arguments
    /// * `terms` - Input `terms` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_terms_of_service(mut self, terms: impl Into<String>) -> Self {
        self.terms_of_service = Some(terms.into());
        self
    }

    /// Sets optional contact metadata.
    ///
    /// # Arguments
    /// * `contact` - Input `contact` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_contact(mut self, contact: OpenApiContact) -> Self {
        self.contact = Some(contact);
        self
    }

    /// Sets optional license metadata.
    ///
    /// # Arguments
    /// * `license` - Input `license` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_license(mut self, license: OpenApiLicense) -> Self {
        self.license = Some(license);
        self
    }

    /// Sets the `$self` URI for the OpenAPI document.
    ///
    /// # Arguments
    /// * `self_uri` - Input `self_uri` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_self_uri(mut self, self_uri: impl Into<String>) -> Self {
        self.self_uri = Some(self_uri.into());
        self
    }

    /// Sets top-level external documentation for the generated OpenAPI document.
    ///
    /// # Arguments
    /// * `url` - Input `url` parameter.
    /// * `description` - Input `description` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_external_docs(
        mut self,
        url: impl Into<String>,
        description: Option<String>,
    ) -> Self {
        self.external_docs = Some(ParsedExternalDocs {
            url: url.into(),
            description,
        });
        self
    }

    /// Adds a server definition to the OpenAPI document.
    ///
    /// # Arguments
    /// * `server` - Input `server` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_server(mut self, server: OpenApiServer) -> Self {
        self.servers.push(server);
        self
    }

    /// Replaces the server list for the OpenAPI document.
    ///
    /// # Arguments
    /// * `servers` - Input `servers` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_servers(mut self, servers: Vec<OpenApiServer>) -> Self {
        self.servers = servers;
        self
    }

    /// Adds a tag definition to the OpenAPI document.
    ///
    /// # Arguments
    /// * `tag` - Input `tag` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_tag(mut self, tag: OpenApiTag) -> Self {
        self.tags.push(tag);
        self
    }

    /// Replaces the tag definitions for the OpenAPI document.
    ///
    /// # Arguments
    /// * `tags` - Input `tags` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_tags(mut self, tags: Vec<OpenApiTag>) -> Self {
        self.tags = tags;
        self
    }

    /// Sets top-level security requirements.
    ///
    /// # Arguments
    /// * `security` - Input `security` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_security(
        mut self,
        security: Vec<crate::openapi::parse::models::SecurityRequirementGroup>,
    ) -> Self {
        self.security = security;
        self
    }

    /// Adds a specification extension (`x-...`) to the Paths Object.
    ///
    /// # Arguments
    /// * `key` - Input `key` parameter.
    /// * `value` - Input `value` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_paths_extension(mut self, key: impl Into<String>, value: Value) -> Self {
        self.paths_extensions.insert(key.into(), value);
        self
    }

    /// Replaces the Paths Object extensions to emit.
    ///
    /// # Arguments
    /// * `extensions` - Input `extensions` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_paths_extensions(mut self, extensions: BTreeMap<String, Value>) -> Self {
        self.paths_extensions = extensions;
        self
    }

    /// Adds a specification extension (`x-...`) to the Webhooks Object.
    ///
    /// # Arguments
    /// * `key` - Input `key` parameter.
    /// * `value` - Input `value` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_webhooks_extension(mut self, key: impl Into<String>, value: Value) -> Self {
        self.webhooks_extensions.insert(key.into(), value);
        self
    }

    /// Replaces the Webhooks Object extensions to emit.
    ///
    /// # Arguments
    /// * `extensions` - Input `extensions` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_webhooks_extensions(mut self, extensions: BTreeMap<String, Value>) -> Self {
        self.webhooks_extensions = extensions;
        self
    }

    /// Adds a specification extension (`x-...`) to the OpenAPI root.
    ///
    /// # Arguments
    /// * `key` - Input `key` parameter.
    /// * `value` - Input `value` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_extension(mut self, key: impl Into<String>, value: Value) -> Self {
        self.extensions.insert(key.into(), value);
        self
    }

    /// Replaces the specification extensions to emit at the OpenAPI root.
    ///
    /// # Arguments
    /// * `extensions` - Input `extensions` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_extensions(mut self, extensions: BTreeMap<String, Value>) -> Self {
        self.extensions = extensions;
        self
    }
}

/// Contact metadata for the OpenAPI Info object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiContact {
    /// The identifying name of the contact person/organization.
    pub name: Option<String>,
    /// The URL for the contact information.
    pub url: Option<String>,
    /// The email address of the contact person/organization.
    pub email: Option<String>,
}

impl Default for OpenApiContact {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenApiContact {
    /// Creates an empty contact object.
    pub fn new() -> Self {
        Self {
            name: None,
            url: None,
            email: None,
        }
    }

    /// Sets the contact name.
    ///
    /// # Arguments
    /// * `name` - Input `name` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the contact URL.
    ///
    /// # Arguments
    /// * `url` - Input `url` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Sets the contact email.
    ///
    /// # Arguments
    /// * `email` - Input `email` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }
}

/// License metadata for the OpenAPI Info object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiLicense {
    /// The license name used for the API.
    pub name: String,
    /// Optional SPDX identifier.
    pub identifier: Option<String>,
    /// Optional URL pointing to the license text.
    pub url: Option<String>,
}

impl OpenApiLicense {
    /// Creates a new license with the required name.
    ///
    /// # Arguments
    /// * `name` - Input `name` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            identifier: None,
            url: None,
        }
    }

    /// Sets the license SPDX identifier.
    ///
    /// # Arguments
    /// * `identifier` - Input `identifier` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_identifier(mut self, identifier: impl Into<String>) -> Self {
        self.identifier = Some(identifier.into());
        self
    }

    /// Sets the license URL.
    ///
    /// # Arguments
    /// * `url` - Input `url` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }
}

/// Tag metadata for the OpenAPI `tags` array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiTag {
    /// The tag name.
    pub name: String,
    /// A short summary of the tag.
    pub summary: Option<String>,
    /// A longer description of the tag.
    pub description: Option<String>,
    /// Optional external documentation for the tag.
    pub external_docs: Option<ParsedExternalDocs>,
    /// Optional parent tag name.
    pub parent: Option<String>,
    /// Optional tag kind (e.g., `nav`, `badge`, `audience`).
    pub kind: Option<String>,
}

impl OpenApiTag {
    /// Creates a new tag with the required name.
    ///
    /// # Arguments
    /// * `name` - Input `name` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            summary: None,
            description: None,
            external_docs: None,
            parent: None,
            kind: None,
        }
    }

    /// Sets the tag summary.
    ///
    /// # Arguments
    /// * `summary` - Input `summary` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Sets the tag description.
    ///
    /// # Arguments
    /// * `description` - Input `description` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the tag external documentation.
    ///
    /// # Arguments
    /// * `url` - Input `url` parameter.
    /// * `description` - Input `description` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_external_docs(
        mut self,
        url: impl Into<String>,
        description: Option<String>,
    ) -> Self {
        self.external_docs = Some(ParsedExternalDocs {
            url: url.into(),
            description,
        });
        self
    }

    /// Sets the parent tag reference.
    ///
    /// # Arguments
    /// * `parent` - Input `parent` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    /// Sets the tag kind.
    ///
    /// # Arguments
    /// * `kind` - Input `kind` parameter.
    ///
    /// # Returns
    /// Resulting `Self` value.
    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }
}

/// Server metadata for OpenAPI `servers` (unified with [`ParsedServer`]).
pub type OpenApiServer = ParsedServer;

/// Server variable metadata for templated server URLs (unified with [`ParsedServerVariable`]).
pub type OpenApiServerVariable = ParsedServerVariable;

/// Generates a JSON Schema object from a parsed Rust model (struct or enum).
///
/// # Arguments
///
/// * `model` - The parsed Rust model.
/// * `dialect` - Optional JSON Schema dialect URI to associate with the schema.
pub fn generate_json_schema(model: &ParsedModel, dialect: Option<&str>) -> AppResult<Value> {
    match model {
        ParsedModel::Struct(s) => generate_struct_schema(s, dialect),
        ParsedModel::Enum(e) => generate_enum_schema(e, dialect),
    }
}

/// Executes `build_info_object` (build info object).
///
/// # Arguments
/// * `info` - Input `info` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn build_info_object(info: &OpenApiInfo) -> Value {
    let mut info_obj = Map::new();
    info_obj.insert("title".to_string(), json!(info.title));
    info_obj.insert("version".to_string(), json!(info.version));
    if let Some(summary) = &info.summary {
        info_obj.insert("summary".to_string(), json!(summary));
    }
    if let Some(desc) = &info.description {
        info_obj.insert("description".to_string(), json!(desc));
    }
    if let Some(terms) = &info.terms_of_service {
        info_obj.insert("termsOfService".to_string(), json!(terms));
    }
    if let Some(contact) = &info.contact {
        let mut contact_obj = Map::new();
        if let Some(name) = &contact.name {
            contact_obj.insert("name".to_string(), json!(name));
        }
        if let Some(url) = &contact.url {
            contact_obj.insert("url".to_string(), json!(url));
        }
        if let Some(email) = &contact.email {
            contact_obj.insert("email".to_string(), json!(email));
        }
        if !contact_obj.is_empty() {
            info_obj.insert("contact".to_string(), Value::Object(contact_obj));
        }
    }
    if let Some(license) = &info.license {
        let mut license_obj = Map::new();
        license_obj.insert("name".to_string(), json!(license.name));
        if let Some(identifier) = &license.identifier {
            license_obj.insert("identifier".to_string(), json!(identifier));
        }
        if let Some(url) = &license.url {
            license_obj.insert("url".to_string(), json!(url));
        }
        info_obj.insert("license".to_string(), Value::Object(license_obj));
    }
    Value::Object(info_obj)
}

/// Generates a minimal OpenAPI 3.2 document containing the provided model as a component schema.
///
/// This is intended for Rust -> OpenAPI workflows where a single type is being reflected
/// into an OpenAPI description.
///
/// # Arguments
/// * `model` - Input `model` parameter.
/// * `dialect` - Input `dialect` parameter.
/// * `info` - Input `info` parameter.
///
/// # Returns
/// Resulting `AppResult<Value>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
pub fn generate_openapi_document(
    model: &ParsedModel,
    dialect: Option<&str>,
    info: &OpenApiInfo,
) -> AppResult<Value> {
    validate_openapi_info(info)?;
    let schema = generate_json_schema(model, dialect)?;

    let mut components = Map::new();
    let mut schemas = Map::new();
    schemas.insert(model.name().to_string(), schema);
    components.insert("schemas".to_string(), Value::Object(schemas));

    let mut doc = Map::new();
    doc.insert("openapi".to_string(), json!("3.2.0"));
    insert_extensions(&mut doc, &info.extensions);
    if let Some(self_uri) = &info.self_uri {
        doc.insert("$self".to_string(), json!(self_uri));
    }
    if let Some(d) = dialect {
        doc.insert("jsonSchemaDialect".to_string(), json!(d));
    }
    doc.insert("info".to_string(), build_info_object(info));
    if let Some(ext) = &info.external_docs {
        doc.insert("externalDocs".to_string(), external_docs_value(ext));
    }
    if !info.security.is_empty() {
        doc.insert("security".to_string(), build_security(&info.security));
    }
    if !info.servers.is_empty() {
        doc.insert("servers".to_string(), servers_value(&info.servers));
    }
    if !info.tags.is_empty() {
        let tag_entries = info.tags.iter().map(tag_value).collect::<Vec<_>>();
        doc.insert("tags".to_string(), Value::Array(tag_entries));
    }
    doc.insert("components".to_string(), Value::Object(components));

    Ok(Value::Object(doc))
}

/// Generates an OpenAPI 3.2 document for the provided models and routes.
///
/// This is intended for Rust -> OpenAPI workflows that need both `components/schemas`
/// and `paths`/`webhooks` derived from parsed routes.
///
/// # Arguments
/// * `models` - Input `models` parameter.
/// * `routes` - Input `routes` parameter.
/// * `dialect` - Input `dialect` parameter.
/// * `info` - Input `info` parameter.
///
/// # Returns
/// Resulting `AppResult<Value>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
pub fn generate_openapi_document_with_routes(
    models: &[ParsedModel],
    routes: &[ParsedRoute],
    dialect: Option<&str>,
    info: &OpenApiInfo,
) -> AppResult<Value> {
    validate_openapi_info(info)?;
    let mut doc = Map::new();
    doc.insert("openapi".to_string(), json!("3.2.0"));
    insert_extensions(&mut doc, &info.extensions);
    if let Some(self_uri) = &info.self_uri {
        doc.insert("$self".to_string(), json!(self_uri));
    }
    if let Some(d) = dialect {
        doc.insert("jsonSchemaDialect".to_string(), json!(d));
    }

    doc.insert("info".to_string(), build_info_object(info));
    if let Some(ext) = &info.external_docs {
        doc.insert("externalDocs".to_string(), external_docs_value(ext));
    }
    if !info.security.is_empty() {
        doc.insert("security".to_string(), build_security(&info.security));
    }

    let tag_entries = collect_tag_entries(&info.tags, routes);
    if !tag_entries.is_empty() {
        doc.insert("tags".to_string(), Value::Array(tag_entries));
    }

    let emit_operation_servers = if !info.servers.is_empty() {
        doc.insert("servers".to_string(), servers_value(&info.servers));
        false
    } else {
        let base_paths = collect_route_base_paths(routes);
        if base_paths.len() == 1 {
            let url = base_paths[0].clone();
            doc.insert("servers".to_string(), json!([{ "url": url }]));
            false
        } else {
            true
        }
    };

    let mut components = Map::new();
    if !models.is_empty() {
        let mut schemas = Map::new();
        for model in models {
            schemas.insert(
                model.name().to_string(),
                generate_json_schema(model, dialect)?,
            );
        }
        components.insert("schemas".to_string(), Value::Object(schemas));
    }
    let security_schemes = collect_security_schemes(routes)?;
    if !security_schemes.is_empty() {
        components.insert(
            "securitySchemes".to_string(),
            Value::Object(security_schemes),
        );
    }
    if !components.is_empty() {
        doc.insert("components".to_string(), Value::Object(components));
    }

    let root_security_empty = info.security.is_empty();
    let (paths, webhooks) = build_paths_and_webhooks(
        routes,
        emit_operation_servers,
        root_security_empty,
        &info.paths_extensions,
        &info.webhooks_extensions,
    )?;
    if !paths.is_empty() {
        doc.insert("paths".to_string(), Value::Object(paths));
    }
    if !webhooks.is_empty() {
        doc.insert("webhooks".to_string(), Value::Object(webhooks));
    }

    let has_content = ["components", "paths", "webhooks"]
        .iter()
        .any(|k| doc.contains_key(*k));
    if !has_content {
        return Err(AppError::General(
            "OpenAPI document must define at least one of 'components', 'paths', or 'webhooks'"
                .to_string(),
        ));
    }

    Ok(Value::Object(doc))
}

/// Generates an OpenAPI 3.2 document for the provided models and routes while
/// preserving an existing `components` object.
///
/// This is intended for OpenAPI -> Rust -> OpenAPI workflows where non-schema
/// components (responses, parameters, mediaTypes, etc.) must be preserved, and
/// schema entries must retain raw JSON Schema keywords that are not expressible
/// in Rust types.
///
/// # Arguments
/// * `models` - Input `models` parameter.
/// * `routes` - Input `routes` parameter.
/// * `dialect` - Input `dialect` parameter.
/// * `info` - Input `info` parameter.
/// * `components` - Input `components` parameter.
///
/// # Returns
/// Resulting `AppResult<Value>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
pub fn generate_openapi_document_with_routes_and_components(
    models: &[ParsedModel],
    routes: &[ParsedRoute],
    dialect: Option<&str>,
    info: &OpenApiInfo,
    components: Option<&serde_json::Value>,
) -> AppResult<Value> {
    let mut doc = generate_openapi_document_with_routes(models, routes, dialect, info)?;
    let Some(raw_components) = components.and_then(|c| c.as_object()) else {
        return Ok(doc);
    };

    let generated_components = doc.get("components").and_then(|v| v.as_object()).cloned();

    let mut merged = raw_components.clone();
    if let Some(generated) = generated_components {
        if let Some(gen_schemas) = generated.get("schemas").and_then(|v| v.as_object()) {
            let mut schemas = merged
                .get("schemas")
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_default();
            for (name, schema) in gen_schemas {
                let merged_schema = merge_schema_value(schema.clone(), schemas.get(name));
                schemas.insert(name.clone(), merged_schema);
            }
            merged.insert("schemas".to_string(), Value::Object(schemas));
        }
        for (key, value) in generated {
            if key == "schemas" {
                continue;
            }
            merged.insert(key, value);
        }
    }

    if !merged.is_empty() {
        doc.as_object_mut()
            .expect("doc object")
            .insert("components".to_string(), Value::Object(merged));
    }

    Ok(doc)
}

/// Executes `build_paths_and_webhooks` (build paths and webhooks).
///
/// # Arguments
/// * `routes` - Input `routes` parameter.
/// * `emit_operation_servers` - Input `emit_operation_servers` parameter.
/// * `root_security_empty` - Input `root_security_empty` parameter.
/// * `paths_extensions` - Input `paths_extensions` parameter.
/// * `webhooks_extensions` - Input `webhooks_extensions` parameter.
///
/// # Returns
/// Resulting `AppResult<(Map<String, Value>, Map<String, Value>)>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn build_paths_and_webhooks(
    routes: &[ParsedRoute],
    emit_operation_servers: bool,
    root_security_empty: bool,
    paths_extensions: &BTreeMap<String, Value>,
    webhooks_extensions: &BTreeMap<String, Value>,
) -> AppResult<(Map<String, Value>, Map<String, Value>)> {
    let mut paths = Map::new();
    let mut webhooks = Map::new();

    for route in routes {
        let target = match route.kind {
            RouteKind::Path => &mut paths,
            RouteKind::Webhook => &mut webhooks,
        };
        insert_route_operation(target, route, emit_operation_servers, root_security_empty)?;
    }

    insert_extensions(&mut paths, paths_extensions);
    insert_extensions(&mut webhooks, webhooks_extensions);

    Ok((paths, webhooks))
}

/// Executes `insert_route_operation` (insert route operation).
///
/// # Arguments
/// * `target` - Input `target` parameter.
/// * `route` - Input `route` parameter.
/// * `emit_operation_servers` - Input `emit_operation_servers` parameter.
/// * `root_security_empty` - Input `root_security_empty` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn insert_route_operation(
    target: &mut Map<String, Value>,
    route: &ParsedRoute,
    emit_operation_servers: bool,
    root_security_empty: bool,
) -> AppResult<()> {
    let mut path_item = match target.remove(&route.path) {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };

    merge_path_extensions(&mut path_item, &route.path_extensions, &route.path)?;
    if let Some(summary) = &route.path_summary {
        path_item
            .entry("summary".to_string())
            .or_insert_with(|| json!(summary));
    }
    if let Some(desc) = &route.path_description {
        path_item
            .entry("description".to_string())
            .or_insert_with(|| json!(desc));
    }
    if let Some(servers) = route.path_servers.as_ref().filter(|s| !s.is_empty()) {
        path_item
            .entry("servers".to_string())
            .or_insert_with(|| servers_value(servers));
    }
    if !route.path_params.is_empty() && !path_item.contains_key("parameters") {
        let params = route
            .path_params
            .iter()
            .map(build_parameter)
            .collect::<Vec<_>>();
        path_item.insert("parameters".to_string(), Value::Array(params));
    }

    let op = build_operation(route, emit_operation_servers, root_security_empty);
    let method_key = route.method.to_ascii_lowercase();
    if is_reserved_method(&method_key) {
        path_item.insert(method_key, op);
        target.insert(route.path.clone(), Value::Object(path_item));
        return Ok(());
    }

    path_item
        .entry("additionalOperations".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("additionalOperations map")
        .insert(route.method.clone(), op);
    target.insert(route.path.clone(), Value::Object(path_item));
    Ok(())
}

/// Executes `merge_path_extensions` (merge path extensions).
///
/// # Arguments
/// * `path_item` - Input `path_item` parameter.
/// * `extensions` - Input `extensions` parameter.
/// * `path` - Input `path` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn merge_path_extensions(
    path_item: &mut Map<String, Value>,
    extensions: &BTreeMap<String, Value>,
    path: &str,
) -> AppResult<()> {
    for (key, value) in extensions {
        if !key.starts_with("x-") {
            return Err(AppError::General(format!(
                "Path item extensions for '{}' must start with 'x-': '{}'",
                path, key
            )));
        }
        if let Some(existing) = path_item.get(key) {
            if existing != value {
                return Err(AppError::General(format!(
                    "Path item extension '{}' for '{}' conflicts across operations",
                    key, path
                )));
            }
            continue;
        }
        path_item.insert(key.clone(), value.clone());
    }
    Ok(())
}

/// Executes `build_operation` (build operation).
///
/// # Arguments
/// * `route` - Input `route` parameter.
/// * `emit_operation_servers` - Input `emit_operation_servers` parameter.
/// * `root_security_empty` - Input `root_security_empty` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn build_operation(
    route: &ParsedRoute,
    emit_operation_servers: bool,
    root_security_empty: bool,
) -> Value {
    let mut op = Map::new();
    let operation_id = route.operation_id.as_ref().unwrap_or(&route.handler_name);
    op.insert("operationId".to_string(), json!(operation_id));
    let op_summary = route.operation_summary.as_ref().or_else(|| {
        if route.path_summary.is_none() {
            route.summary.as_ref()
        } else {
            None
        }
    });
    let op_description = route.operation_description.as_ref().or_else(|| {
        if route.path_description.is_none() {
            route.description.as_ref()
        } else {
            None
        }
    });
    if let Some(summary) = op_summary {
        op.insert("summary".to_string(), json!(summary));
    }
    if let Some(desc) = op_description {
        op.insert("description".to_string(), json!(desc));
    }
    if route.deprecated {
        op.insert("deprecated".to_string(), json!(true));
    }
    if !route.tags.is_empty() {
        op.insert(
            "tags".to_string(),
            Value::Array(route.tags.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(ext) = &route.external_docs {
        op.insert("externalDocs".to_string(), external_docs_value(ext));
    }
    insert_extensions(&mut op, &route.extensions);

    if let Some(servers) = route.servers_override.as_ref() {
        if !servers.is_empty() {
            op.insert("servers".to_string(), servers_value(servers));
        }
    } else if emit_operation_servers && route.path_servers.is_none() {
        if let Some(base_path) = route.base_path.as_ref() {
            op.insert("servers".to_string(), json!([{ "url": base_path }]));
        }
    }

    let op_params: Vec<&RouteParam> = route
        .params
        .iter()
        .filter(|param| !route.path_params.iter().any(|p| p == *param))
        .collect();
    if !op_params.is_empty() {
        let params = op_params
            .into_iter()
            .map(build_parameter)
            .collect::<Vec<_>>();
        op.insert("parameters".to_string(), Value::Array(params));
    }

    if let Some(raw) = route.raw_request_body.as_ref() {
        op.insert(
            "requestBody".to_string(),
            merge_request_body(raw, route.request_body.as_ref()),
        );
    } else if let Some(body) = &route.request_body {
        op.insert("requestBody".to_string(), build_request_body(body));
    }

    op.insert("responses".to_string(), build_responses(route));

    let emit_security =
        route.security_defined || (root_security_empty && !route.security.is_empty());
    if emit_security {
        op.insert("security".to_string(), build_security(&route.security));
    }

    if !route.callbacks.is_empty() {
        op.insert(
            "callbacks".to_string(),
            build_callbacks(&route.callbacks, root_security_empty),
        );
    }

    Value::Object(op)
}

/// Executes `validate_openapi_info` (validate openapi info).
///
/// # Arguments
/// * `info` - Input `info` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_openapi_info(info: &OpenApiInfo) -> AppResult<()> {
    if let Some(license) = &info.license {
        if license.identifier.is_some() && license.url.is_some() {
            return Err(AppError::General(
                "OpenAPI license must not set both 'identifier' and 'url'".to_string(),
            ));
        }
    }
    validate_extension_keys(&info.paths_extensions, "paths")?;
    validate_extension_keys(&info.webhooks_extensions, "webhooks")?;
    Ok(())
}

/// Executes `validate_extension_keys` (validate extension keys).
///
/// # Arguments
/// * `extensions` - Input `extensions` parameter.
/// * `context` - Input `context` parameter.
///
/// # Returns
/// Resulting `AppResult<()>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn validate_extension_keys(extensions: &BTreeMap<String, Value>, context: &str) -> AppResult<()> {
    for key in extensions.keys() {
        if !key.starts_with("x-") {
            return Err(AppError::General(format!(
                "{} extensions must start with 'x-': '{}'",
                context, key
            )));
        }
    }
    Ok(())
}

/// Executes `insert_extensions` (insert extensions).
///
/// # Arguments
/// * `target` - Input `target` parameter.
/// * `extensions` - Input `extensions` parameter.
fn insert_extensions(target: &mut Map<String, Value>, extensions: &BTreeMap<String, Value>) {
    for (key, value) in extensions {
        if key.starts_with("x-") {
            target.insert(key.clone(), value.clone());
        }
    }
}

/// Executes `servers_value` (servers value).
///
/// # Arguments
/// * `servers` - Input `servers` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn servers_value(servers: &[OpenApiServer]) -> Value {
    let entries = servers.iter().map(server_value).collect::<Vec<_>>();
    Value::Array(entries)
}

/// Executes `server_value` (server value).
///
/// # Arguments
/// * `server` - Input `server` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn server_value(server: &OpenApiServer) -> Value {
    let mut obj = Map::new();
    obj.insert("url".to_string(), json!(server.url));
    if let Some(desc) = &server.description {
        obj.insert("description".to_string(), json!(desc));
    }
    if let Some(name) = &server.name {
        obj.insert("name".to_string(), json!(name));
    }
    if !server.variables.is_empty() {
        let mut vars = Map::new();
        for (var_name, var) in &server.variables {
            vars.insert(var_name.clone(), server_variable_value(var));
        }
        obj.insert("variables".to_string(), Value::Object(vars));
    }
    Value::Object(obj)
}

/// Executes `server_variable_value` (server variable value).
///
/// # Arguments
/// * `variable` - Input `variable` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn server_variable_value(variable: &OpenApiServerVariable) -> Value {
    let mut obj = Map::new();
    obj.insert("default".to_string(), json!(variable.default));
    if let Some(values) = &variable.enum_values {
        obj.insert("enum".to_string(), json!(values));
    }
    if let Some(desc) = &variable.description {
        obj.insert("description".to_string(), json!(desc));
    }
    Value::Object(obj)
}

/// Executes `collect_tag_entries` (collect tag entries).
///
/// # Arguments
/// * `info_tags` - Input `info_tags` parameter.
/// * `routes` - Input `routes` parameter.
///
/// # Returns
/// Resulting `Vec<Value>` value.
fn collect_tag_entries(info_tags: &[OpenApiTag], routes: &[ParsedRoute]) -> Vec<Value> {
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for tag in info_tags {
        if seen.insert(tag.name.clone()) {
            entries.push(tag_value(tag));
        }
    }

    for name in collect_route_tags(routes) {
        if seen.insert(name.clone()) {
            entries.push(json!({ "name": name }));
        }
    }

    entries
}

/// Executes `tag_value` (tag value).
///
/// # Arguments
/// * `tag` - Input `tag` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn tag_value(tag: &OpenApiTag) -> Value {
    let mut obj = Map::new();
    obj.insert("name".to_string(), json!(tag.name));
    if let Some(summary) = &tag.summary {
        obj.insert("summary".to_string(), json!(summary));
    }
    if let Some(desc) = &tag.description {
        obj.insert("description".to_string(), json!(desc));
    }
    if let Some(ext) = &tag.external_docs {
        obj.insert("externalDocs".to_string(), external_docs_value(ext));
    }
    if let Some(parent) = &tag.parent {
        obj.insert("parent".to_string(), json!(parent));
    }
    if let Some(kind) = &tag.kind {
        obj.insert("kind".to_string(), json!(kind));
    }
    Value::Object(obj)
}

/// Collects unique, non-empty tag names from parsed routes.
///
/// # Arguments
/// * `routes` - Input `routes` parameter.
///
/// # Returns
/// Resulting `Vec<String>` value.
fn collect_route_tags(routes: &[ParsedRoute]) -> Vec<String> {
    let mut tags = std::collections::BTreeSet::new();
    for route in routes {
        for tag in &route.tags {
            if !tag.trim().is_empty() {
                tags.insert(tag.clone());
            }
        }
    }
    tags.into_iter().collect()
}

/// Executes `collect_security_schemes` (collect security schemes).
///
/// # Arguments
/// * `routes` - Input `routes` parameter.
///
/// # Returns
/// Resulting `AppResult<Map<String, Value>>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or processing fails.
fn collect_security_schemes(routes: &[ParsedRoute]) -> AppResult<Map<String, Value>> {
    let mut seen: std::collections::HashMap<String, SecuritySchemeInfo> =
        std::collections::HashMap::new();

    let all_groups = routes.iter().flat_map(|route| {
        route
            .security
            .iter()
            .chain(route.callbacks.iter().flat_map(|cb| cb.security.iter()))
    });

    for group in all_groups {
        for req in &group.schemes {
            let Some(info) = req.scheme.as_ref() else {
                continue;
            };
            if let Some(existing) = seen.get(&req.scheme_name) {
                if existing != info {
                    return Err(crate::error::AppError::General(format!(
                        "Conflicting security scheme definitions for '{}'",
                        req.scheme_name
                    )));
                }
                continue;
            }
            seen.insert(req.scheme_name.clone(), info.clone());
        }
    }

    let mut map = Map::new();
    for (name, info) in seen {
        if let Some(value) = security_scheme_value(&info) {
            map.insert(name, value);
        }
    }
    Ok(map)
}

/// Executes `security_scheme_value` (security scheme value).
///
/// # Arguments
/// * `info` - Input `info` parameter.
///
/// # Returns
/// Resulting `Option<Value>` value.
fn security_scheme_value(info: &SecuritySchemeInfo) -> Option<Value> {
    let mut obj = Map::new();
    match &info.kind {
        SecuritySchemeKind::ApiKey { name, in_loc } => {
            let in_str = api_key_location(in_loc)?;
            obj.insert("type".to_string(), json!("apiKey"));
            obj.insert("name".to_string(), json!(name));
            obj.insert("in".to_string(), json!(in_str));
        }
        SecuritySchemeKind::Http {
            scheme,
            bearer_format,
        } => {
            obj.insert("type".to_string(), json!("http"));
            obj.insert("scheme".to_string(), json!(scheme));
            if let Some(fmt) = bearer_format {
                obj.insert("bearerFormat".to_string(), json!(fmt));
            }
        }
        SecuritySchemeKind::MutualTls => {
            obj.insert("type".to_string(), json!("mutualTLS"));
        }
        SecuritySchemeKind::OAuth2 {
            flows,
            oauth2_metadata_url,
        } => {
            obj.insert("type".to_string(), json!("oauth2"));
            obj.insert("flows".to_string(), oauth_flows_value(flows));
            if let Some(url) = oauth2_metadata_url {
                obj.insert("oauth2MetadataUrl".to_string(), json!(url));
            }
        }
        SecuritySchemeKind::OpenIdConnect {
            open_id_connect_url,
        } => {
            obj.insert("type".to_string(), json!("openIdConnect"));
            obj.insert("openIdConnectUrl".to_string(), json!(open_id_connect_url));
        }
    }

    if let Some(desc) = &info.description {
        obj.insert("description".to_string(), json!(desc));
    }
    if info.deprecated {
        obj.insert("deprecated".to_string(), json!(true));
    }

    Some(Value::Object(obj))
}

/// Executes `oauth_flows_value` (oauth flows value).
///
/// # Arguments
/// * `flows` - Input `flows` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn oauth_flows_value(flows: &crate::openapi::parse::models::OAuthFlows) -> Value {
    let mut map = Map::new();
    if let Some(flow) = flows.implicit.as_ref() {
        map.insert("implicit".to_string(), oauth_flow_value(flow));
    }
    if let Some(flow) = flows.password.as_ref() {
        map.insert("password".to_string(), oauth_flow_value(flow));
    }
    if let Some(flow) = flows.client_credentials.as_ref() {
        map.insert("clientCredentials".to_string(), oauth_flow_value(flow));
    }
    if let Some(flow) = flows.authorization_code.as_ref() {
        map.insert("authorizationCode".to_string(), oauth_flow_value(flow));
    }
    if let Some(flow) = flows.device_authorization.as_ref() {
        map.insert("deviceAuthorization".to_string(), oauth_flow_value(flow));
    }
    Value::Object(map)
}

/// Executes `oauth_flow_value` (oauth flow value).
///
/// # Arguments
/// * `flow` - Input `flow` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn oauth_flow_value(flow: &crate::openapi::parse::models::OAuthFlow) -> Value {
    let mut map = Map::new();
    if let Some(url) = &flow.authorization_url {
        map.insert("authorizationUrl".to_string(), json!(url));
    }
    if let Some(url) = &flow.device_authorization_url {
        map.insert("deviceAuthorizationUrl".to_string(), json!(url));
    }
    if let Some(url) = &flow.token_url {
        map.insert("tokenUrl".to_string(), json!(url));
    }
    if let Some(url) = &flow.refresh_url {
        map.insert("refreshUrl".to_string(), json!(url));
    }
    map.insert("scopes".to_string(), json!(flow.scopes));
    Value::Object(map)
}

/// Executes `api_key_location` (api key location).
///
/// # Arguments
/// * `source` - Input `source` parameter.
///
/// # Returns
/// Resulting `Option<&'static str>` value.
fn api_key_location(source: &ParamSource) -> Option<&'static str> {
    match source {
        ParamSource::Header => Some("header"),
        ParamSource::Query => Some("query"),
        ParamSource::Cookie => Some("cookie"),
        _ => None,
    }
}

/// Collects unique, non-empty base paths from parsed routes.
///
/// # Arguments
/// * `routes` - Input `routes` parameter.
///
/// # Returns
/// Resulting `Vec<String>` value.
fn collect_route_base_paths(routes: &[ParsedRoute]) -> Vec<String> {
    let mut paths = std::collections::BTreeSet::new();
    for route in routes {
        if route.servers_override.is_some() {
            continue;
        }
        if let Some(base_path) = route.base_path.as_ref() {
            let trimmed = base_path.trim();
            if !trimmed.is_empty() {
                paths.insert(trimmed.to_string());
            }
        }
    }
    paths.into_iter().collect()
}

/// Executes `insert_example_value` (insert example value).
///
/// # Arguments
/// * `target` - Input `target` parameter.
/// * `example` - Input `example` parameter.
fn insert_example_value(target: &mut Map<String, Value>, example: &ExampleValue) {
    let has_meta = example.summary.is_some() || example.description.is_some();
    let mut example_obj = Map::new();
    if let Some(summary) = &example.summary {
        example_obj.insert("summary".to_string(), json!(summary));
    }
    if let Some(description) = &example.description {
        example_obj.insert("description".to_string(), json!(description));
    }

    match example.kind {
        crate::openapi::parse::models::ExampleKind::Data => {
            if has_meta {
                example_obj.insert("dataValue".to_string(), example.value.clone());
                target.insert("examples".to_string(), json!({ "example": example_obj }));
            } else {
                target.insert("example".to_string(), example.value.clone());
            }
        }
        crate::openapi::parse::models::ExampleKind::Serialized => {
            let serialized = match &example.value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            example_obj.insert("serializedValue".to_string(), json!(serialized));
            target.insert("examples".to_string(), json!({ "example": example_obj }));
        }
        crate::openapi::parse::models::ExampleKind::External => {
            let external = match &example.value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            example_obj.insert("externalValue".to_string(), json!(external));
            target.insert("examples".to_string(), json!({ "example": example_obj }));
        }
    }
}

/// Executes `build_parameter` (build parameter).
///
/// # Arguments
/// * `param` - Input `param` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn build_parameter(param: &RouteParam) -> Value {
    let mut obj = Map::new();
    obj.insert("name".to_string(), json!(param.name));
    obj.insert("in".to_string(), json!(param_source_str(&param.source)));

    if let Some(desc) = &param.description {
        obj.insert("description".to_string(), json!(desc));
    }
    if param.deprecated {
        obj.insert("deprecated".to_string(), json!(true));
    }
    if param.allow_empty_value {
        obj.insert("allowEmptyValue".to_string(), json!(true));
    }
    if let Some(example) = &param.example {
        insert_example_value(&mut obj, example);
    }
    insert_extensions(&mut obj, &param.extensions);

    let (schema, optional) = schema_from_rust_type(&param.ty);
    let schema = merge_schema_value(schema, param.raw_schema.as_ref());
    let required = matches!(param.source, ParamSource::Path) || !optional;
    obj.insert("required".to_string(), json!(required));

    let content_media =
        param
            .content_media_type
            .as_ref()
            .map(|m| m.as_str())
            .or(match param.source {
                ParamSource::QueryString => Some("application/x-www-form-urlencoded"),
                _ => None,
            });

    if let Some(media_type) = content_media {
        obj.insert(
            "content".to_string(),
            json!({ media_type: { "schema": schema } }),
        );
    } else {
        obj.insert("schema".to_string(), schema);
        if let Some(style) = &param.style {
            obj.insert("style".to_string(), json!(style_to_str(style)));
        }
        obj.insert("explode".to_string(), json!(param.explode));
        if param.allow_reserved {
            obj.insert("allowReserved".to_string(), json!(true));
        }
    }

    Value::Object(obj)
}

/// Executes `merge_json_objects` (merge json objects).
///
/// # Arguments
/// * `base` - Input `base` parameter.
/// * `raw` - Input `raw` parameter.
///
/// # Returns
/// Resulting `Map<String, Value>` value.
fn merge_json_objects(
    mut base: Map<String, Value>,
    raw: &Map<String, Value>,
) -> Map<String, Value> {
    for (key, raw_val) in raw {
        match base.get_mut(key) {
            None => {
                base.insert(key.clone(), raw_val.clone());
            }
            Some(base_val) => {
                if let (Value::Object(base_obj), Value::Object(raw_obj)) = (base_val, raw_val) {
                    *base_obj = merge_json_objects(base_obj.clone(), raw_obj);
                }
            }
        }
    }
    base
}

/// Executes `merge_schema_value` (merge schema value).
///
/// # Arguments
/// * `generated` - Input `generated` parameter.
/// * `raw` - Input `raw` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn merge_schema_value(generated: Value, raw: Option<&Value>) -> Value {
    let Some(raw_val) = raw else {
        return generated;
    };
    match (generated, raw_val) {
        (Value::Object(gen_obj), Value::Object(raw_obj)) => {
            Value::Object(merge_json_objects(gen_obj, raw_obj))
        }
        (gen, _) => gen,
    }
}

/// Executes `merge_response_entry_with_raw` (merge response entry with raw).
///
/// # Arguments
/// * `generated` - Input `generated` parameter.
/// * `raw` - Input `raw` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn merge_response_entry_with_raw(generated: Value, raw: Option<&Value>) -> Value {
    merge_schema_value(generated, raw)
}

/// Executes `merge_request_body` (merge request body).
///
/// # Arguments
/// * `raw` - Input `raw` parameter.
/// * `body` - Input `body` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn merge_request_body(
    raw: &Value,
    body: Option<&crate::openapi::parse::models::RequestBodyDefinition>,
) -> Value {
    let Some(body) = body else {
        return raw.clone();
    };
    let generated = build_request_body(body);
    merge_schema_value(generated, Some(raw))
}

/// Executes `build_request_body` (build request body).
///
/// # Arguments
/// * `body` - Input `body` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn build_request_body(body: &crate::openapi::parse::models::RequestBodyDefinition) -> Value {
    let mut obj = Map::new();
    if let Some(desc) = &body.description {
        obj.insert("description".to_string(), json!(desc));
    }
    if body.required {
        obj.insert("required".to_string(), json!(true));
    }

    let mut content = Map::new();
    content.insert(
        body.media_type.clone(),
        Value::Object(build_request_body_media(body)),
    );
    obj.insert("content".to_string(), Value::Object(content));

    Value::Object(obj)
}

/// Executes `build_request_body_media` (build request body media).
///
/// # Arguments
/// * `body` - Input `body` parameter.
///
/// # Returns
/// Resulting `Map<String, Value>` value.
fn build_request_body_media(
    body: &crate::openapi::parse::models::RequestBodyDefinition,
) -> Map<String, Value> {
    let (schema, _) = schema_from_rust_type(&body.ty);
    let mut media = Map::new();
    if is_sequential_media_type(&body.media_type) {
        if let Some(item_schema) = schema_item_schema(&schema) {
            media.insert("itemSchema".to_string(), item_schema);
        }
    }
    media.insert("schema".to_string(), schema);
    if let Some(example) = &body.example {
        insert_example_value(&mut media, example);
    }

    if let Some(encoding) = body.encoding.as_ref() {
        let mut enc_map = Map::new();
        for (prop, info) in encoding {
            enc_map.insert(prop.clone(), encoding_info_value(info));
        }
        if !enc_map.is_empty() {
            media.insert("encoding".to_string(), Value::Object(enc_map));
        }
    }

    if let Some(prefix) = body.prefix_encoding.as_ref() {
        let items = prefix.iter().map(encoding_info_value).collect::<Vec<_>>();
        if !items.is_empty() {
            media.insert("prefixEncoding".to_string(), Value::Array(items));
        }
    }

    if let Some(item) = body.item_encoding.as_ref() {
        media.insert("itemEncoding".to_string(), encoding_info_value(item));
    }

    media
}

/// Executes `build_responses` (build responses).
///
/// # Arguments
/// * `route` - Input `route` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn build_responses(route: &ParsedRoute) -> Value {
    let status = route.response_status.as_deref().unwrap_or("200");
    let entry = build_response_entry(route);

    if let Some(raw) = route.raw_responses.as_ref() {
        if let Some(raw_obj) = raw.as_object() {
            let mut merged = raw_obj.clone();
            if route.response_status.is_some() || merged.is_empty() {
                let raw_entry = merged.get(status).cloned();
                let merged_entry = merge_response_entry_with_raw(entry, raw_entry.as_ref());
                merged.insert(status.to_string(), merged_entry);
            }
            return Value::Object(merged);
        }
    }

    let mut responses = Map::new();
    responses.insert(status.to_string(), entry);
    Value::Object(responses)
}

/// Executes `build_response_object` (build response object).
///
/// # Arguments
/// * `response_summary` - Input `response_summary` parameter.
/// * `response_description` - Input `response_description` parameter.
/// * `response_media_type` - Input `response_media_type` parameter.
/// * `response_type` - Input `response_type` parameter.
/// * `response_example` - Input `response_example` parameter.
/// * `response_headers` - Input `response_headers` parameter.
/// * `response_links` - Input `response_links` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn build_response_object(
    response_summary: Option<&str>,
    response_description: Option<&str>,
    response_media_type: Option<&str>,
    response_type: Option<&str>,
    response_example: Option<&crate::openapi::parse::models::ExampleValue>,
    response_headers: &[crate::openapi::parse::models::ResponseHeader],
    response_links: Option<&[ParsedLink]>,
) -> Value {
    let mut resp = Map::new();
    if let Some(summary) = response_summary {
        resp.insert("summary".to_string(), json!(summary));
    }
    let description = response_description.unwrap_or("OK");
    let media_type = response_media_type.unwrap_or("application/json");

    resp.insert("description".to_string(), json!(description));

    if response_type.is_some() || response_example.is_some() {
        let mut media = Map::new();
        if let Some(ty) = response_type {
            let (schema, _) = schema_from_rust_type(ty);
            if is_sequential_media_type(media_type) {
                if let Some(item_schema) = schema_item_schema(&schema) {
                    media.insert("itemSchema".to_string(), item_schema);
                }
            }
            media.insert("schema".to_string(), schema);
        }
        if let Some(example) = response_example {
            insert_example_value(&mut media, example);
        }
        let mut content = Map::new();
        content.insert(media_type.to_string(), Value::Object(media));
        resp.insert("content".to_string(), Value::Object(content));
    }

    if !response_headers.is_empty() {
        let mut headers = Map::new();
        for header in response_headers {
            let (schema, _) = schema_from_rust_type(&header.ty);
            let mut h = Map::new();
            if let Some(desc) = &header.description {
                h.insert("description".to_string(), json!(desc));
            }
            if header.required {
                h.insert("required".to_string(), json!(true));
            }
            if header.deprecated {
                h.insert("deprecated".to_string(), json!(true));
            }
            if let Some(media_type) = &header.content_media_type {
                let mut media = Map::new();
                media.insert("schema".to_string(), schema);
                if let Some(example) = &header.example {
                    insert_example_value(&mut media, example);
                }
                let mut content = Map::new();
                content.insert(media_type.clone(), Value::Object(media));
                h.insert("content".to_string(), Value::Object(content));
            } else {
                h.insert("schema".to_string(), schema);
                if let Some(example) = &header.example {
                    insert_example_value(&mut h, example);
                }
                if let Some(style) = &header.style {
                    h.insert("style".to_string(), json!(style_to_str(style)));
                }
                if let Some(explode) = header.explode {
                    h.insert("explode".to_string(), json!(explode));
                }
            }
            insert_extensions(&mut h, &header.extensions);
            headers.insert(header.name.clone(), Value::Object(h));
        }
        resp.insert("headers".to_string(), Value::Object(headers));
    }

    if let Some(links) = response_links {
        if !links.is_empty() {
            let mut link_map = Map::new();
            for link in links {
                link_map.insert(link.name.clone(), build_link(link));
            }
            resp.insert("links".to_string(), Value::Object(link_map));
        }
    }

    Value::Object(resp)
}

/// Executes `build_response_entry` (build response entry).
///
/// # Arguments
/// * `route` - Input `route` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn build_response_entry(route: &ParsedRoute) -> Value {
    build_response_object(
        route.response_summary.as_deref(),
        route.response_description.as_deref(),
        route.response_media_type.as_deref(),
        route.response_type.as_deref(),
        route.response_example.as_ref(),
        &route.response_headers,
        route.response_links.as_deref(),
    )
}

/// Executes `build_link` (build link).
///
/// # Arguments
/// * `link` - Input `link` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn build_link(link: &ParsedLink) -> Value {
    let mut obj = Map::new();
    if let Some(desc) = &link.description {
        obj.insert("description".to_string(), json!(desc));
    }
    if let Some(op_id) = &link.operation_id {
        obj.insert("operationId".to_string(), json!(op_id));
    }
    if let Some(op_ref) = &link.operation_ref {
        obj.insert("operationRef".to_string(), json!(op_ref));
    }
    if !link.parameters.is_empty() {
        let mut params = Map::new();
        for (key, value) in &link.parameters {
            let val = match value {
                LinkParamValue::Expression(expr) => json!(expr.as_str()),
                LinkParamValue::Literal(lit) => lit.clone(),
            };
            params.insert(key.clone(), val);
        }
        obj.insert("parameters".to_string(), Value::Object(params));
    }
    if let Some(body) = &link.request_body {
        let val = match body {
            LinkRequestBody::Expression(expr) => json!(expr.as_str()),
            LinkRequestBody::Literal(lit) => lit.clone(),
        };
        obj.insert("requestBody".to_string(), val);
    }
    if let Some(server) = &link.server {
        obj.insert("server".to_string(), server_value(server));
    } else if let Some(server_url) = &link.server_url {
        obj.insert("server".to_string(), json!({ "url": server_url }));
    }
    Value::Object(obj)
}

/// Executes `build_callbacks` (build callbacks).
///
/// # Arguments
/// * `callbacks` - Input `callbacks` parameter.
/// * `root_security_empty` - Input `root_security_empty` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn build_callbacks(
    callbacks: &[crate::openapi::parse::models::ParsedCallback],
    root_security_empty: bool,
) -> Value {
    let mut cb_map = Map::new();

    for callback in callbacks {
        let mut expr_map = match cb_map.remove(&callback.name) {
            Some(Value::Object(map)) => map,
            _ => Map::new(),
        };

        let expr_key = callback.expression.as_str().to_string();
        let mut path_item = match expr_map.remove(&expr_key) {
            Some(Value::Object(map)) => map,
            _ => Map::new(),
        };

        if !callback.path_params.is_empty() && !path_item.contains_key("parameters") {
            let params = callback
                .path_params
                .iter()
                .map(build_parameter)
                .collect::<Vec<_>>();
            path_item.insert("parameters".to_string(), Value::Array(params));
        }

        insert_callback_operation(&mut path_item, callback, root_security_empty);
        expr_map.insert(expr_key, Value::Object(path_item));
        cb_map.insert(callback.name.clone(), Value::Object(expr_map));
    }

    Value::Object(cb_map)
}

/// Executes `insert_callback_operation` (insert callback operation).
///
/// # Arguments
/// * `path_item` - Input `path_item` parameter.
/// * `callback` - Input `callback` parameter.
/// * `root_security_empty` - Input `root_security_empty` parameter.
fn insert_callback_operation(
    path_item: &mut Map<String, Value>,
    callback: &crate::openapi::parse::models::ParsedCallback,
    root_security_empty: bool,
) {
    let emit_security = [
        callback.security_defined,
        [root_security_empty, !callback.security.is_empty()]
            .into_iter()
            .all(|b| b),
    ]
    .into_iter()
    .any(|b| b);
    let op = build_callback_operation(callback, emit_security);
    let method_key = callback.method.to_ascii_lowercase();
    if is_reserved_method(&method_key) {
        path_item.insert(method_key, op);
        return;
    }

    path_item
        .entry("additionalOperations".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("additionalOperations map")
        .insert(callback.method.clone(), op);
}

/// Executes `build_callback_operation` (build callback operation).
///
/// # Arguments
/// * `callback` - Input `callback` parameter.
/// * `emit_security` - Input `emit_security` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn build_callback_operation(
    callback: &crate::openapi::parse::models::ParsedCallback,
    emit_security: bool,
) -> Value {
    let mut op = Map::new();

    if !callback.params.is_empty() {
        let params = callback
            .params
            .iter()
            .map(build_parameter)
            .collect::<Vec<_>>();
        op.insert("parameters".to_string(), Value::Array(params));
    }

    if let Some(body) = &callback.request_body {
        op.insert("requestBody".to_string(), build_request_body(body));
    }

    if emit_security {
        op.insert("security".to_string(), build_security(&callback.security));
    }

    op.insert("responses".to_string(), build_callback_responses(callback));

    Value::Object(op)
}

/// Executes `build_callback_responses` (build callback responses).
///
/// # Arguments
/// * `callback` - Input `callback` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn build_callback_responses(callback: &crate::openapi::parse::models::ParsedCallback) -> Value {
    let mut responses = Map::new();
    let status = callback.response_status.as_deref().unwrap_or("200");
    let resp = build_response_object(
        callback.response_summary.as_deref(),
        callback.response_description.as_deref(),
        callback.response_media_type.as_deref(),
        callback.response_type.as_deref(),
        callback.response_example.as_ref(),
        &callback.response_headers,
        None,
    );
    responses.insert(status.to_string(), resp);
    Value::Object(responses)
}

/// Executes `build_security` (build security).
///
/// # Arguments
/// * `requirements` - Input `requirements` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn build_security(
    requirements: &[crate::openapi::parse::models::SecurityRequirementGroup],
) -> Value {
    let mut list = Vec::new();
    for group in requirements {
        if group.schemes.is_empty() {
            list.push(Value::Object(Map::new()));
            continue;
        }
        let mut map = Map::new();
        for req in &group.schemes {
            let scopes = req
                .scopes
                .iter()
                .cloned()
                .map(Value::String)
                .collect::<Vec<_>>();
            map.insert(req.scheme_name.clone(), Value::Array(scopes));
        }
        list.push(Value::Object(map));
    }
    Value::Array(list)
}

/// Executes `encoding_info_value` (encoding info value).
///
/// # Arguments
/// * `info` - Input `info` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn encoding_info_value(info: &crate::openapi::parse::models::EncodingInfo) -> Value {
    let mut obj = Map::new();
    if let Some(ct) = &info.content_type {
        obj.insert("contentType".to_string(), json!(ct));
    }
    if !info.headers.is_empty() {
        let mut headers = Map::new();
        for (name, ty) in &info.headers {
            let (schema, _) = schema_from_rust_type(ty);
            headers.insert(name.clone(), json!({ "schema": schema }));
        }
        obj.insert("headers".to_string(), Value::Object(headers));
    }
    if let Some(style) = &info.style {
        obj.insert("style".to_string(), json!(style_to_str(style)));
    }
    if let Some(explode) = info.explode {
        obj.insert("explode".to_string(), json!(explode));
    }
    if let Some(allow_reserved) = info.allow_reserved {
        obj.insert("allowReserved".to_string(), json!(allow_reserved));
    }
    if let Some(nested) = info.encoding.as_ref() {
        let mut map = Map::new();
        for (name, enc) in nested {
            map.insert(name.clone(), encoding_info_value(enc));
        }
        if !map.is_empty() {
            obj.insert("encoding".to_string(), Value::Object(map));
        }
    }
    if let Some(prefix) = info.prefix_encoding.as_ref() {
        let items = prefix.iter().map(encoding_info_value).collect::<Vec<_>>();
        if !items.is_empty() {
            obj.insert("prefixEncoding".to_string(), Value::Array(items));
        }
    }
    if let Some(item) = info.item_encoding.as_ref() {
        obj.insert("itemEncoding".to_string(), encoding_info_value(item));
    }
    Value::Object(obj)
}

/// Executes `schema_from_rust_type` (schema from rust type).
///
/// # Arguments
/// * `ty` - Input `ty` parameter.
///
/// # Returns
/// Resulting `(Value, bool)` value.
fn schema_from_rust_type(ty: &str) -> (Value, bool) {
    crate::type_mapping::map_rust_type_to_unwrapped_openapi_schema(ty)
}

/// Executes `param_source_str` (param source str).
///
/// # Arguments
/// * `source` - Input `source` parameter.
///
/// # Returns
/// Resulting `&'static str` value.
fn param_source_str(source: &ParamSource) -> &'static str {
    match source {
        ParamSource::Path => "path",
        ParamSource::Query => "query",
        ParamSource::QueryString => "querystring",
        ParamSource::Header => "header",
        ParamSource::Cookie => "cookie",
        ParamSource::FormData => "formData",
        ParamSource::Body => "body",
    }
}

/// Executes `style_to_str` (style to str).
///
/// # Arguments
/// * `style` - Input `style` parameter.
///
/// # Returns
/// Resulting `&'static str` value.
fn style_to_str(style: &ParamStyle) -> &'static str {
    match style {
        ParamStyle::Matrix => "matrix",
        ParamStyle::Label => "label",
        ParamStyle::Form => "form",
        ParamStyle::Cookie => "cookie",
        ParamStyle::Simple => "simple",
        ParamStyle::SpaceDelimited => "spaceDelimited",
        ParamStyle::PipeDelimited => "pipeDelimited",
        ParamStyle::DeepObject => "deepObject",
    }
}

/// Executes `is_reserved_method` (is reserved method).
///
/// # Arguments
/// * `method` - Input `method` parameter.
///
/// # Returns
/// Resulting `bool` value.
fn is_reserved_method(method: &str) -> bool {
    matches!(
        method,
        "get" | "post" | "put" | "delete" | "patch" | "options" | "head" | "trace" | "query"
    )
}

/// Executes `is_sequential_media_type` (is sequential media type).
///
/// # Arguments
/// * `media_type` - Input `media_type` parameter.
///
/// # Returns
/// Resulting `bool` value.
fn is_sequential_media_type(media_type: &str) -> bool {
    let mt = crate::openapi::parse::models::MediaType::parse(media_type);
    [
        mt.is_sequential_json(),
        mt.is_event_stream(),
        matches!(
            mt.essence().as_str(),
            "multipart/mixed" | "multipart/byteranges"
        ),
    ]
    .into_iter()
    .any(|b| b)
}

/// Executes `schema_item_schema` (schema item schema).
///
/// # Arguments
/// * `schema` - Input `schema` parameter.
///
/// # Returns
/// Resulting `Option<Value>` value.
fn schema_item_schema(schema: &Value) -> Option<Value> {
    schema
        .as_object()
        .filter(|obj| obj.get("type").and_then(|v| v.as_str()) == Some("array"))
        .and_then(|obj| obj.get("items").cloned())
}

/// Generates schema for a struct.
///
/// # Arguments
/// * `struct_def` - Input `struct_def` parameter.
/// * `dialect` - Input `dialect` parameter.
///
/// # Returns
/// Resulting `AppResult<Value>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
fn generate_struct_schema(struct_def: &ParsedStruct, dialect: Option<&str>) -> AppResult<Value> {
    let mut schema = Map::new();

    // 0. Dialect (if provided)
    if let Some(d) = dialect {
        schema.insert("$schema".to_string(), json!(d));
    }

    // 1. Basic Metadata
    let title = struct_def
        .rename
        .clone()
        .unwrap_or_else(|| struct_def.name.clone());
    schema.insert("title".to_string(), json!(title));

    if let Some(desc) = &struct_def.description {
        schema.insert("description".to_string(), json!(desc));
    }

    if struct_def.is_deprecated {
        schema.insert("deprecated".to_string(), json!(true));
    }
    if let Some(ext) = &struct_def.external_docs {
        schema.insert("externalDocs".to_string(), external_docs_value(ext));
    }

    // 2. Check if it is a Tuple Struct vs Named Struct
    // A Tuple struct in Rust is parsed with fields named "0", "1", "2"...
    // We detect if ALL fields are numeric.
    let is_tuple = !struct_def.fields.is_empty()
        && struct_def
            .fields
            .iter()
            .all(|f| f.name.chars().all(char::is_numeric));

    if is_tuple {
        schema.insert("type".to_string(), json!("array"));
        let mut prefix_items = Vec::new();

        // Sort fields by index to ensure correct order
        let mut sorted_fields = struct_def.fields.clone();
        sorted_fields.sort_by_key(|f| f.name.parse::<usize>().unwrap_or(0));

        for field in sorted_fields {
            if field.is_skipped {
                continue;
            }
            let (_, field_schema, _) = process_field(&field, struct_def.rename_all.as_ref());
            prefix_items.push(field_schema);
        }

        schema.insert("prefixItems".to_string(), Value::Array(prefix_items));
        // Tuple structs are fixed length in Rust
        schema.insert("items".to_string(), json!(false));
    } else {
        schema.insert("type".to_string(), json!("object"));

        let mut properties = Map::new();
        let mut required = Vec::new();

        for field in &struct_def.fields {
            if field.is_skipped {
                continue;
            }

            let (json_name, mut field_schema, is_optional) =
                process_field(field, struct_def.rename_all.as_ref());

            let obj = field_schema.as_object_mut().expect("field schema object");
            if field.is_deprecated {
                obj.insert("deprecated".to_string(), json!(true));
            }
            if let Some(ext) = &field.external_docs {
                obj.insert("externalDocs".to_string(), external_docs_value(ext));
            }

            properties.insert(json_name.clone(), field_schema);

            if !is_optional {
                required.push(json_name);
            }
        }

        schema.insert("properties".to_string(), Value::Object(properties));

        if !required.is_empty() {
            let required_json: Vec<Value> = required.into_iter().map(Value::String).collect();
            schema.insert("required".to_string(), Value::Array(required_json));
        }

        if struct_def.deny_unknown_fields {
            schema.insert("additionalProperties".to_string(), json!(false));
        }
    }

    Ok(Value::Object(schema))
}

/// Generates schema for an enum using `oneOf`.
///
/// # Arguments
/// * `enum_def` - Input `enum_def` parameter.
/// * `dialect` - Input `dialect` parameter.
///
/// # Returns
/// Resulting `AppResult<Value>` value.
///
/// # Errors
/// Returns an error if validation, parsing, or execution fails.
fn generate_enum_schema(enum_def: &ParsedEnum, dialect: Option<&str>) -> AppResult<Value> {
    let mut schema = Map::new();

    // 0. Dialect
    if let Some(d) = dialect {
        schema.insert("$schema".to_string(), json!(d));
    }

    let title = enum_def
        .rename
        .clone()
        .unwrap_or_else(|| enum_def.name.clone());
    schema.insert("title".to_string(), json!(title));

    if let Some(desc) = &enum_def.description {
        schema.insert("description".to_string(), json!(desc));
    }

    if enum_def.is_deprecated {
        schema.insert("deprecated".to_string(), json!(true));
    }
    if let Some(ext) = &enum_def.external_docs {
        schema.insert("externalDocs".to_string(), external_docs_value(ext));
    }

    let mut one_of = Vec::new();

    for variant in &enum_def.variants {
        let variant_name = variant.rename.clone().unwrap_or_else(|| {
            enum_def
                .rename_all
                .as_ref()
                .map(|rule| rule.apply(&variant.name))
                .unwrap_or_else(|| variant.name.clone())
        });

        // Determine variant schema
        let mut sub_schema = if let Some(ty) = &variant.ty {
            crate::type_mapping::map_base_type_to_openapi_schema(ty)
        } else {
            // Unit variant -> Enum::A -> "A"
            json!({ "type": "string", "const": variant_name })
        };

        if variant.is_deprecated {
            sub_schema
                .as_object_mut()
                .expect("variant schema object")
                .insert("deprecated".to_string(), json!(true));
        }

        one_of.push(sub_schema);
    }

    // Handle Untagged vs Tagged
    schema.insert("oneOf".to_string(), Value::Array(one_of));

    if let Some(tag) = &enum_def.tag {
        // Tagged enum: add discriminator hint (with optional mapping)
        let mut discriminator = Map::new();
        discriminator.insert("propertyName".to_string(), json!(tag));
        if let Some(mapping) = enum_def
            .discriminator_mapping
            .as_ref()
            .filter(|m| !m.is_empty())
        {
            discriminator.insert("mapping".to_string(), json!(mapping));
        }
        if let Some(default_mapping) = enum_def
            .discriminator_default_mapping
            .as_ref()
            .filter(|d| !d.is_empty())
        {
            discriminator.insert("defaultMapping".to_string(), json!(default_mapping));
        }
        schema.insert("discriminator".to_string(), Value::Object(discriminator));
    }

    Ok(Value::Object(schema))
}

/// Executes `external_docs_value` (external docs value).
///
/// # Arguments
/// * `ext` - Input `ext` parameter.
///
/// # Returns
/// Resulting `Value` value.
fn external_docs_value(ext: &ParsedExternalDocs) -> Value {
    let mut map = Map::new();
    map.insert("url".to_string(), json!(ext.url));
    if let Some(desc) = &ext.description {
        map.insert("description".to_string(), json!(desc));
    }
    Value::Object(map)
}

/// Processes a single field to determine its JSON name, schema, and optionality.
///
/// # Arguments
/// * `field` - Input `field` parameter.
/// * `rename_all` - Input `rename_all` parameter.
///
/// # Returns
/// Resulting `(String, Value, bool)` value.
fn process_field(field: &ParsedField, rename_all: Option<&RenameRule>) -> (String, Value, bool) {
    // 1. Determine Name
    let name = field
        .rename
        .clone()
        .or_else(|| rename_all.map(|rule| rule.apply(&field.name)))
        .unwrap_or_else(|| field.name.clone());

    // 2. Parse Type via unified type_mapping subsystem
    let (mut schema, is_optional) = crate::type_mapping::map_rust_type_to_openapi_schema(&field.ty);

    let obj = schema.as_object_mut().expect("field schema object");
    if let Some(desc) = &field.description {
        obj.insert("description".to_string(), json!(desc));
    }
    if field.is_read_only {
        obj.insert("readOnly".to_string(), json!(true));
    }
    if field.is_write_only {
        obj.insert("writeOnly".to_string(), json!(true));
    }

    (name, schema, is_optional)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classes::parse::{ParsedField, ParsedVariant};
    use crate::openapi::parse::models::{
        BodyFormat, EncodingInfo, ExampleValue, ParsedCallback, RequestBodyDefinition,
        ResponseHeader, RuntimeExpression,
    };
    use crate::openapi::parse::models::{
        ParamSource, ParamStyle, ParsedRoute, ParsedServer, ParsedServerVariable, RouteKind,
        RouteParam, SecurityRequirement,
    };
    use std::collections::BTreeMap;

    fn make_struct(name: &str, fields: Vec<ParsedField>) -> ParsedStruct {
        ParsedStruct {
            name: name.into(),
            description: Some("Test Struct".into()),
            rename: None,
            rename_all: None,
            fields,
            is_deprecated: false,
            deny_unknown_fields: false,
            external_docs: None,
        }
    }

    fn make_field(name: &str, ty: &str, rename: Option<&str>) -> ParsedField {
        ParsedField {
            name: name.into(),
            ty: ty.into(),
            description: Some("A field".into()),
            is_skipped: false,
            is_read_only: false,
            is_write_only: false,
            rename: rename.map(|s| s.into()),
            is_deprecated: false,
            external_docs: None,
        }
    }

    #[test]
    fn test_generate_simple_schema() {
        let fields = vec![
            make_field("id", "i32", None),
            make_field("active", "bool", None),
        ];

        let def = ParsedModel::Struct(make_struct("User", fields));
        let schema = generate_json_schema(&def, None).expect("Failed to generate json schema");

        let props = schema["properties"]
            .as_object()
            .expect("Failed to get as object");
        assert_eq!(props["id"]["type"], "integer");
        assert_eq!(props["active"]["type"], "boolean");
        assert!(!schema
            .as_object()
            .expect("Failed to get as object")
            .contains_key("$schema"));
    }

    #[test]
    fn test_generate_tuple_schema() {
        let fields = vec![
            make_field("0", "i32", None),
            make_field("1", "String", None),
        ];

        let def = ParsedModel::Struct(make_struct("Point", fields));
        let schema = generate_json_schema(&def, None).expect("Failed to generate json schema");

        assert_eq!(schema["type"], "array");
        assert_eq!(schema["items"], false);
        let prefix_items = schema["prefixItems"]
            .as_array()
            .expect("Failed to get as array");
        assert_eq!(prefix_items.len(), 2);
        assert_eq!(prefix_items[0]["type"], "integer");
        assert_eq!(prefix_items[1]["type"], "string");
    }

    #[test]
    fn test_generate_schema_with_dialect() {
        let fields = vec![make_field("id", "i32", None)];
        let def = ParsedModel::Struct(make_struct("User", fields));
        let dialect = "https://spec.openapis.org/oas/3.1/dialect/base";

        let schema =
            generate_json_schema(&def, Some(dialect)).expect("Failed to generate json schema");

        assert_eq!(schema["$schema"], dialect);
        assert_eq!(schema["title"], "User");
    }

    #[test]
    fn test_generate_schema_rename_all_and_deny_unknown() {
        let fields = vec![
            make_field("user_id", "i32", None),
            make_field("display_name", "String", None),
        ];
        let mut s = make_struct("UserProfile", fields);
        s.rename_all = Some(RenameRule::CamelCase);
        s.deny_unknown_fields = true;

        let schema = generate_json_schema(&ParsedModel::Struct(s), None)
            .expect("Failed to generate json schema");
        let props = schema["properties"]
            .as_object()
            .expect("Failed to get as object");
        assert!(props.contains_key("userId"));
        assert!(props.contains_key("displayName"));
        assert_eq!(schema["additionalProperties"], false);
    }

    #[test]
    fn test_generate_openapi_document_wraps_schema() {
        let fields = vec![make_field("id", "i32", None)];
        let def = ParsedModel::Struct(make_struct("User", fields));
        let contact = OpenApiContact::new()
            .with_name("API Support")
            .with_email("support@example.com");
        let license = OpenApiLicense::new("Apache 2.0").with_identifier("Apache-2.0");
        let info = OpenApiInfo::new("Test API", "1.0.0")
            .with_summary("Short summary")
            .with_description("Docs")
            .with_terms_of_service("https://example.com/terms")
            .with_contact(contact)
            .with_license(license);
        let dialect = "https://spec.openapis.org/oas/3.1/dialect/base";

        let doc = generate_openapi_document(&def, Some(dialect), &info)
            .expect("Failed to generate openapi document");
        assert_eq!(doc["openapi"], "3.2.0");
        assert_eq!(doc["jsonSchemaDialect"], dialect);
        assert_eq!(doc["info"]["title"], "Test API");
        assert_eq!(doc["info"]["version"], "1.0.0");
        assert_eq!(doc["info"]["summary"], "Short summary");
        assert_eq!(doc["info"]["description"], "Docs");
        assert_eq!(doc["info"]["termsOfService"], "https://example.com/terms");
        assert_eq!(doc["info"]["contact"]["name"], "API Support");
        assert_eq!(doc["info"]["contact"]["email"], "support@example.com");
        assert_eq!(doc["info"]["license"]["name"], "Apache 2.0");
        assert_eq!(doc["info"]["license"]["identifier"], "Apache-2.0");
        assert_eq!(doc["components"]["schemas"]["User"]["title"], "User");
    }

    #[test]
    fn test_generate_openapi_document_includes_self_uri() {
        let fields = vec![make_field("id", "i32", None)];
        let def = ParsedModel::Struct(make_struct("User", fields));
        let info = OpenApiInfo::new("Test API", "1.0.0")
            .with_self_uri("https://example.com/openapi.yaml")
            .with_external_docs("https://example.com/docs", Some("Root docs".to_string()));

        let doc = generate_openapi_document(&def, None, &info)
            .expect("Failed to generate openapi document");
        assert_eq!(doc["$self"], "https://example.com/openapi.yaml");
        assert_eq!(doc["externalDocs"]["url"], "https://example.com/docs");
        assert_eq!(doc["externalDocs"]["description"], "Root docs");
    }

    #[test]
    fn test_generate_openapi_document_includes_extensions() {
        let fields = vec![make_field("id", "i32", None)];
        let def = ParsedModel::Struct(make_struct("User", fields));
        let mut extensions = BTreeMap::new();
        extensions.insert("x-root".to_string(), serde_json::json!({"mode": "test"}));

        let info = OpenApiInfo::new("Test API", "1.0.0").with_extensions(extensions);
        let doc = generate_openapi_document(&def, None, &info)
            .expect("Failed to generate openapi document");
        assert_eq!(doc["x-root"]["mode"], "test");
    }

    #[test]
    fn test_generate_openapi_document_rejects_license_identifier_and_url() {
        let fields = vec![make_field("id", "i32", None)];
        let def = ParsedModel::Struct(make_struct("User", fields));
        let license = OpenApiLicense::new("Apache 2.0")
            .with_identifier("Apache-2.0")
            .with_url("https://example.com/license");
        let info = OpenApiInfo::new("Test API", "1.0.0").with_license(license);

        let err = generate_openapi_document(&def, None, &info).expect_err("expected error");
        assert!(format!("{err}").contains("identifier"));
    }

    #[test]
    fn test_generate_openapi_document_rejects_invalid_paths_extension_key() {
        let fields = vec![make_field("id", "i32", None)];
        let def = ParsedModel::Struct(make_struct("User", fields));
        let info =
            OpenApiInfo::new("Test API", "1.0.0").with_paths_extension("paths-meta", json!(true));

        let err = generate_openapi_document(&def, None, &info).expect_err("expected error");
        assert!(format!("{err}").contains("paths extensions"));
    }

    #[test]
    fn test_generate_openapi_document_with_tag_metadata() {
        let info = OpenApiInfo::new("Test API", "1.0.0").with_tag(
            OpenApiTag::new("accounts")
                .with_summary("Account ops")
                .with_description("Operations for account resources")
                .with_kind("nav")
                .with_parent("root"),
        );
        let route = ParsedRoute {
            path: "/accounts".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "list_accounts".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: true,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec!["accounts".to_string(), "extra".to_string()],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let tags = doc["tags"].as_array().expect("Failed to get as array");
        let first = &tags[0];
        assert_eq!(first["name"], "accounts");
        assert_eq!(first["summary"], "Account ops");
        assert_eq!(first["description"], "Operations for account resources");
        assert_eq!(first["kind"], "nav");
        assert_eq!(first["parent"], "root");
        assert!(tags.iter().any(|t| t["name"] == "extra"));
    }

    #[test]
    fn test_generate_openapi_document_with_explicit_servers() {
        let info = OpenApiInfo::new("Test API", "1.0.0").with_server(
            OpenApiServer::new("https://api.example.com/v1")
                .with_name("prod")
                .with_description("Production"),
        );
        let route = ParsedRoute {
            path: "/users".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: Some("/api/v1".to_string()),

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "list_users".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: true,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        assert_eq!(doc["servers"][0]["url"], "https://api.example.com/v1");
        assert_eq!(doc["servers"][0]["name"], "prod");
        assert_eq!(doc["servers"][0]["description"], "Production");
        assert!(doc["paths"]["/users"]["get"].get("servers").is_none());
    }

    #[test]
    fn test_generate_map_schema() {
        let fields = vec![make_field("tags", "HashMap<String, i32>", None)];
        let def = ParsedModel::Struct(make_struct("Tagged", fields));
        let schema = generate_json_schema(&def, None).expect("Failed to generate json schema");

        let props = schema["properties"]
            .as_object()
            .expect("Failed to get as object");
        let tags = &props["tags"];
        assert_eq!(tags["type"], "object");
        assert_eq!(tags["additionalProperties"]["type"], "integer");
    }

    #[test]
    fn test_generate_nested_map_schema() {
        let fields = vec![make_field("meta", "BTreeMap<String, Vec<String>>", None)];
        let def = ParsedModel::Struct(make_struct("Meta", fields));
        let schema = generate_json_schema(&def, None).expect("Failed to generate json schema");

        let props = schema["properties"]
            .as_object()
            .expect("Failed to get as object");
        let meta = &props["meta"];
        assert_eq!(meta["type"], "object");
        assert_eq!(meta["additionalProperties"]["type"], "array");
        assert_eq!(meta["additionalProperties"]["items"]["type"], "string");
    }

    #[test]
    fn test_generate_external_docs_on_schema_and_field() {
        let mut fields = vec![make_field("id", "i32", None)];
        fields[0].external_docs = Some(ParsedExternalDocs {
            url: "https://example.com/field".to_string(),
            description: Some("Field docs".to_string()),
        });

        let mut s = make_struct("DocUser", fields);
        s.external_docs = Some(ParsedExternalDocs {
            url: "https://example.com/schema".to_string(),
            description: Some("Schema docs".to_string()),
        });

        let schema = generate_json_schema(&ParsedModel::Struct(s), None)
            .expect("Failed to generate json schema");
        assert_eq!(schema["externalDocs"]["url"], "https://example.com/schema");
        assert_eq!(schema["externalDocs"]["description"], "Schema docs");

        let props = schema["properties"]
            .as_object()
            .expect("Failed to get as object");
        let id = &props["id"];
        assert_eq!(id["externalDocs"]["url"], "https://example.com/field");
        assert_eq!(id["externalDocs"]["description"], "Field docs");
    }

    #[test]
    fn test_generate_read_write_only_fields() {
        let mut read_only = make_field("read_only", "String", None);
        read_only.is_read_only = true;
        let mut write_only = make_field("write_only", "String", None);
        write_only.is_write_only = true;

        let def = ParsedModel::Struct(make_struct("Access", vec![read_only, write_only]));
        let schema = generate_json_schema(&def, None).expect("Failed to generate json schema");
        let props = schema["properties"]
            .as_object()
            .expect("Failed to get as object");
        assert_eq!(props["read_only"]["readOnly"], true);
        assert_eq!(props["write_only"]["writeOnly"], true);
    }

    #[test]
    fn test_optional_map_not_required() {
        let fields = vec![make_field(
            "labels",
            "Option<HashMap<String, String>>",
            None,
        )];
        let def = ParsedModel::Struct(make_struct("OptionalMap", fields));
        let schema = generate_json_schema(&def, None).expect("Failed to generate json schema");

        assert!(schema.get("required").is_none());
        let labels = &schema["properties"]["labels"];
        let types = labels["type"].as_array().expect("Failed to get as array");
        assert!(types
            .iter()
            .any(|t| matches!(t, Value::String(s) if s == "null")));
    }

    #[test]
    fn test_optional_string_nullable_schema() {
        let fields = vec![make_field("nickname", "Option<String>", None)];
        let def = ParsedModel::Struct(make_struct("NullableUser", fields));
        let schema = generate_json_schema(&def, None).expect("Failed to generate json schema");

        let nickname = &schema["properties"]["nickname"];
        let types = nickname["type"].as_array().expect("Failed to get as array");
        assert!(types.iter().any(|t| t == "null"));
        assert!(types.iter().any(|t| t == "string"));
    }

    #[test]
    fn test_optional_ref_schema_uses_anyof() {
        let fields = vec![make_field("owner", "Option<User>", None)];
        let def = ParsedModel::Struct(make_struct("Owned", fields));
        let schema = generate_json_schema(&def, None).expect("Failed to generate json schema");

        let owner = &schema["properties"]["owner"];
        assert!(owner.get("anyOf").is_some());
    }

    #[test]
    fn test_generate_enum_schema() {
        let en = ParsedEnum {
            name: "Pet".into(),
            description: None,
            rename: None,
            rename_all: None,
            tag: Some("type".into()),
            untagged: false,
            is_deprecated: false,
            external_docs: None,
            variants: vec![
                ParsedVariant {
                    name: "Cat".into(),
                    ty: Some("CatInfo".into()),
                    description: None,
                    rename: None,
                    aliases: None,
                    is_deprecated: false,
                },
                ParsedVariant {
                    name: "Dog".into(),
                    ty: Some("DogInfo".into()),
                    description: None,
                    rename: None,
                    aliases: None,
                    is_deprecated: false,
                },
            ],
            discriminator_mapping: None,
            discriminator_default_mapping: None,
        };

        let schema = generate_json_schema(&ParsedModel::Enum(en), None)
            .expect("Failed to generate json schema");
        assert!(schema["oneOf"].is_array());
        assert!(schema["discriminator"].is_object());
        assert_eq!(schema["discriminator"]["propertyName"], "type");
        assert!(!schema
            .as_object()
            .expect("Failed to get as object")
            .contains_key("$schema"));
    }

    #[test]
    fn test_generate_enum_schema_with_rename_all() {
        let en = ParsedEnum {
            name: "Color".into(),
            description: None,
            rename: None,
            rename_all: Some(RenameRule::KebabCase),
            tag: None,
            untagged: true,
            is_deprecated: false,
            external_docs: None,
            variants: vec![
                ParsedVariant {
                    name: "RedBlue".into(),
                    ty: None,
                    description: None,
                    rename: None,
                    aliases: None,
                    is_deprecated: false,
                },
                ParsedVariant {
                    name: "Green".into(),
                    ty: None,
                    description: None,
                    rename: None,
                    aliases: None,
                    is_deprecated: false,
                },
            ],
            discriminator_mapping: None,
            discriminator_default_mapping: None,
        };

        let schema = generate_json_schema(&ParsedModel::Enum(en), None)
            .expect("Failed to generate json schema");
        let one_of = schema["oneOf"].as_array().expect("Failed to get as array");
        assert_eq!(one_of[0]["const"], "red-blue");
        assert_eq!(one_of[1]["const"], "green");
    }

    #[test]
    fn test_generate_struct_title_uses_rename() {
        let fields = vec![make_field("id", "i32", None)];
        let mut s = make_struct("User", fields);
        s.rename = Some("UserModel".to_string());

        let schema = generate_json_schema(&ParsedModel::Struct(s), None)
            .expect("Failed to generate json schema");
        assert_eq!(schema["title"], "UserModel");
    }

    #[test]
    fn test_generate_enum_schema_with_discriminator_mapping() {
        let mut mapping = BTreeMap::new();
        mapping.insert("cat".to_string(), "#/components/schemas/Cat".to_string());
        mapping.insert("dog".to_string(), "#/components/schemas/Dog".to_string());

        let en = ParsedEnum {
            name: "Pet".into(),
            description: None,
            rename: None,
            rename_all: None,
            tag: Some("kind".into()),
            untagged: false,
            is_deprecated: false,
            external_docs: None,
            variants: vec![],
            discriminator_mapping: Some(mapping.clone()),
            discriminator_default_mapping: Some("OtherPet".to_string()),
        };

        let schema = generate_json_schema(&ParsedModel::Enum(en), None)
            .expect("Failed to generate json schema");
        assert_eq!(schema["discriminator"]["propertyName"], "kind");
        assert_eq!(schema["discriminator"]["mapping"], json!(mapping));
        assert_eq!(schema["discriminator"]["defaultMapping"], "OtherPet");
    }

    #[test]
    fn test_generate_enum_schema_with_dialect() {
        let en = ParsedEnum {
            name: "Status".into(),
            description: None,
            rename: None,
            rename_all: None,
            tag: None,
            untagged: false,
            is_deprecated: false,
            external_docs: None,
            variants: vec![],
            discriminator_mapping: None,
            discriminator_default_mapping: None,
        };
        let dialect = "https://json-schema.org/draft/2020-12/schema";
        let schema = generate_json_schema(&ParsedModel::Enum(en), Some(dialect))
            .expect("Failed to generate json schema");
        assert_eq!(schema["$schema"], dialect);
        assert_eq!(schema["title"], "Status");
    }

    #[test]
    fn test_generate_binary_vec_u8_schema() {
        let fields = vec![make_field("payload", "Vec<u8>", None)];
        let def = ParsedModel::Struct(make_struct("Upload", fields));
        let schema = generate_json_schema(&def, None).expect("Failed to generate json schema");

        let payload = &schema["properties"]["payload"];
        assert_eq!(payload["type"], "string");
        assert_eq!(payload["contentEncoding"], "base64");
        assert_eq!(payload["contentMediaType"], "application/octet-stream");
    }

    #[test]
    fn test_generate_binary_bytes_schema() {
        let fields = vec![make_field("data", "bytes::Bytes", None)];
        let def = ParsedModel::Struct(make_struct("Blob", fields));
        let schema = generate_json_schema(&def, None).expect("Failed to generate json schema");

        let data = &schema["properties"]["data"];
        assert_eq!(data["type"], "string");
        assert_eq!(data["contentEncoding"], "base64");
    }

    #[test]
    fn test_generate_binary_optional_not_required() {
        let fields = vec![make_field("payload", "Option<Vec<u8>>", None)];
        let def = ParsedModel::Struct(make_struct("Upload", fields));
        let schema = generate_json_schema(&def, None).expect("Failed to generate json schema");

        assert!(schema.get("required").is_none());
    }

    #[test]
    fn test_generate_openapi_document_with_routes_paths() {
        let model = ParsedModel::Struct(make_struct("User", vec![make_field("id", "i32", None)]));
        let info =
            OpenApiInfo::new("Test API", "1.0.0").with_self_uri("https://example.com/openapi.yaml");

        let param = RouteParam {
            name: "id".to_string(),
            description: None,
            source: ParamSource::Path,
            ty: "Uuid".to_string(),
            content_media_type: None,
            style: None,
            explode: false,
            allow_reserved: false,
            deprecated: false,
            allow_empty_value: false,
            example: None,
            raw_schema: None,
            extensions: BTreeMap::new(),
        };

        let route = ParsedRoute {
            path: "/users/{id}".to_string(),
            summary: Some("Get user".to_string()),
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "get_user".to_string(),
            operation_id: Some("GetUserById".to_string()),
            params: vec![param],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: Some("User".to_string()),
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec!["users".to_string()],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[model], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");

        assert_eq!(doc["$self"], "https://example.com/openapi.yaml");
        assert_eq!(doc["tags"][0]["name"], "users");
        assert_eq!(
            doc["paths"]["/users/{id}"]["get"]["operationId"],
            "GetUserById"
        );
        assert_eq!(
            doc["paths"]["/users/{id}"]["get"]["parameters"][0]["required"],
            true
        );
        assert_eq!(
            doc["paths"]["/users/{id}"]["get"]["responses"]["200"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/User"
        );
        assert!(doc["components"]["schemas"]["User"].is_object());
    }

    #[test]
    fn test_generate_openapi_document_preserves_components() {
        let model = ParsedModel::Struct(make_struct("User", vec![make_field("id", "i32", None)]));
        let info = OpenApiInfo::new("Test API", "1.0.0");

        let route = ParsedRoute {
            path: "/users".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "list_users".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: Some("User".to_string()),
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let raw_components = json!({
            "schemas": {
                "Legacy": { "type": "string" }
            },
            "responses": {
                "NotFound": { "description": "missing" }
            },
            "examples": {
                "Sample": { "value": { "id": 1 } }
            }
        });

        let doc = generate_openapi_document_with_routes_and_components(
            &[model],
            &[route],
            None,
            &info,
            Some(&raw_components),
        )
        .expect("Failed to generate openapi document with routes and components");

        assert!(doc["components"]["schemas"]["Legacy"].is_object());
        assert!(doc["components"]["schemas"]["User"].is_object());
        assert_eq!(
            doc["components"]["responses"]["NotFound"]["description"],
            "missing"
        );
        assert!(doc["components"]["examples"]["Sample"].is_object());
    }

    #[test]
    fn test_generate_openapi_document_merges_component_schema_keywords() {
        let model =
            ParsedModel::Struct(make_struct("User", vec![make_field("id", "String", None)]));
        let info = OpenApiInfo::new("Test API", "1.0.0");

        let route = ParsedRoute {
            path: "/users".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "list_users".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: Some("User".to_string()),
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let raw_components = json!({
            "schemas": {
                "User": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "contentSchema": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string" }
                                }
                            }
                        }
                    },
                    "if": { "properties": { "id": { "type": "string" } } },
                    "then": { "required": ["id"] },
                    "else": { "not": { "required": ["id"] } },
                    "dependentSchemas": {
                        "id": { "properties": { "meta": { "type": "string" } } }
                    },
                    "unevaluatedProperties": false
                }
            }
        });

        let doc = generate_openapi_document_with_routes_and_components(
            &[model],
            &[route],
            None,
            &info,
            Some(&raw_components),
        )
        .expect("Failed to generate openapi document with routes and components");

        let user_schema = &doc["components"]["schemas"]["User"];
        assert_eq!(user_schema["properties"]["id"]["type"], "string");
        assert!(user_schema["properties"]["id"]["contentSchema"].is_object());
        assert!(user_schema.get("if").is_some());
        assert!(user_schema.get("else").is_some());
        assert!(user_schema.get("dependentSchemas").is_some());
        assert_eq!(user_schema["unevaluatedProperties"], false);
    }

    #[test]
    fn test_generate_openapi_document_with_path_item_metadata_and_params() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let path_param = RouteParam {
            name: "id".into(),
            description: None,
            source: ParamSource::Path,
            ty: "String".into(),
            content_media_type: None,
            style: None,
            explode: false,
            deprecated: false,
            allow_empty_value: false,
            allow_reserved: false,
            example: None,
            raw_schema: None,
            extensions: BTreeMap::new(),
        };
        let query_param = RouteParam {
            name: "q".into(),
            description: None,
            source: ParamSource::Query,
            ty: "String".into(),
            content_media_type: None,
            style: None,
            explode: false,
            deprecated: false,
            allow_empty_value: false,
            allow_reserved: false,
            example: None,
            raw_schema: None,
            extensions: BTreeMap::new(),
        };
        let mut path_extensions = BTreeMap::new();
        path_extensions.insert("x-path-meta".to_string(), json!({"owner": "api"}));

        let route = ParsedRoute {
            path: "/items/{id}".to_string(),
            summary: Some("Op summary".into()),
            description: Some("Op description".into()),
            path_summary: Some("Path summary".into()),
            path_description: Some("Path description".into()),
            path_extensions,
            operation_summary: Some("Op summary".into()),
            operation_description: Some("Op description".into()),
            base_path: None,
            path_servers: Some(vec![ParsedServer {
                url: "https://api.example.com/v2".into(),
                description: None,
                name: None,
                variables: BTreeMap::new(),
            }]),
            servers_override: None,
            method: "GET".to_string(),
            handler_name: "get_item".to_string(),
            operation_id: None,
            params: vec![path_param.clone(), query_param.clone()],
            path_params: vec![path_param.clone()],
            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let path_item = &doc["paths"]["/items/{id}"];
        assert_eq!(path_item["summary"], "Path summary");
        assert_eq!(path_item["description"], "Path description");
        assert_eq!(path_item["x-path-meta"]["owner"], "api");
        assert_eq!(path_item["servers"][0]["url"], "https://api.example.com/v2");
        let path_params = path_item["parameters"]
            .as_array()
            .expect("Failed to get as array");
        assert_eq!(path_params[0]["name"], "id");
        assert_eq!(path_params[0]["in"], "path");

        let op = &path_item["get"];
        assert_eq!(op["summary"], "Op summary");
        assert_eq!(op["description"], "Op description");
        assert!(op.get("servers").is_none());
        let op_params = op["parameters"].as_array().expect("Failed to get as array");
        assert!(op_params.iter().any(|p| p["name"] == "q"));
        assert!(!op_params.iter().any(|p| p["name"] == "id"));
    }

    #[test]
    fn test_generate_response_header_with_content() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let route = ParsedRoute {
            path: "/status".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "get_status".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: Some("OK".to_string()),
            response_media_type: Some("text/plain".to_string()),
            response_example: None,
            response_headers: vec![ResponseHeader {
                name: "X-Rate-Limit".to_string(),
                description: Some("limit".to_string()),
                required: false,
                deprecated: false,
                style: None,
                explode: None,
                ty: "i32".to_string(),
                content_media_type: Some("text/plain".to_string()),
                example: Some(ExampleValue::serialized(serde_json::json!("10"))),
                extensions: BTreeMap::new(),
            }],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let header = &doc["paths"]["/status"]["get"]["responses"]["200"]["headers"]["X-Rate-Limit"];
        assert!(header.get("content").is_some());
        assert_eq!(header["content"]["text/plain"]["schema"]["type"], "integer");
        assert!(header["content"]["text/plain"].get("examples").is_some());
    }

    #[test]
    fn test_generate_response_header_schema_metadata() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let route = ParsedRoute {
            path: "/status".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "get_status".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: Some("OK".to_string()),
            response_media_type: None,
            response_example: None,
            response_headers: vec![ResponseHeader {
                name: "X-Flag".to_string(),
                description: Some("flag".to_string()),
                required: true,
                deprecated: true,
                style: Some(ParamStyle::Simple),
                explode: Some(true),
                ty: "String".to_string(),
                content_media_type: None,
                example: None,
                extensions: BTreeMap::new(),
            }],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let header = &doc["paths"]["/status"]["get"]["responses"]["200"]["headers"]["X-Flag"];
        assert_eq!(header["required"], true);
        assert_eq!(header["deprecated"], true);
        assert_eq!(header["style"], "simple");
        assert_eq!(header["explode"], true);
    }

    #[test]
    fn test_generate_openapi_parameter_serialized_example() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let param = RouteParam {
            name: "id".to_string(),
            description: None,
            source: ParamSource::Path,
            ty: "Uuid".to_string(),
            content_media_type: None,
            style: None,
            explode: false,
            allow_reserved: false,
            deprecated: false,
            allow_empty_value: false,
            example: Some(ExampleValue::serialized(serde_json::json!("id=123"))),
            raw_schema: None,
            extensions: BTreeMap::new(),
        };
        let route = ParsedRoute {
            path: "/users/{id}".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "get_user".to_string(),
            operation_id: None,
            params: vec![param],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let param_obj = &doc["paths"]["/users/{id}"]["get"]["parameters"][0];
        assert!(param_obj.get("example").is_none());
        assert_eq!(
            param_obj["examples"]["example"]["serializedValue"],
            "id=123"
        );
    }

    #[test]
    fn test_generate_openapi_parameter_example_metadata() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let param = RouteParam {
            name: "status".to_string(),
            description: None,
            source: ParamSource::Query,
            ty: "String".to_string(),
            content_media_type: None,
            style: None,
            explode: false,
            allow_reserved: false,
            deprecated: false,
            allow_empty_value: false,
            example: Some(ExampleValue::data_with_meta(
                serde_json::json!("active"),
                Some("Short summary".to_string()),
                Some("Longer description".to_string()),
            )),
            raw_schema: None,
            extensions: BTreeMap::new(),
        };

        let route = ParsedRoute {
            path: "/users".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "list_users".to_string(),
            operation_id: None,
            params: vec![param],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: Some("200".to_string()),
            response_summary: None,
            response_description: Some("OK".to_string()),
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let param_obj = &doc["paths"]["/users"]["get"]["parameters"][0];
        let example_obj = &param_obj["examples"]["example"];
        assert_eq!(example_obj["summary"], "Short summary");
        assert_eq!(example_obj["description"], "Longer description");
        assert_eq!(example_obj["dataValue"], "active");
    }

    #[test]
    fn test_generate_openapi_parameter_external_example() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let param = RouteParam {
            name: "id".to_string(),
            description: None,
            source: ParamSource::Path,
            ty: "Uuid".to_string(),
            content_media_type: None,
            style: None,
            explode: false,
            allow_reserved: false,
            deprecated: false,
            allow_empty_value: false,
            example: Some(ExampleValue::external(serde_json::json!(
                "https://example.com/examples/id.txt"
            ))),
            raw_schema: None,
            extensions: BTreeMap::new(),
        };
        let route = ParsedRoute {
            path: "/users/{id}".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "get_user".to_string(),
            operation_id: None,
            params: vec![param],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let param_obj = &doc["paths"]["/users/{id}"]["get"]["parameters"][0];
        assert!(param_obj.get("example").is_none());
        assert_eq!(
            param_obj["examples"]["example"]["externalValue"],
            "https://example.com/examples/id.txt"
        );
    }

    #[test]
    fn test_generate_openapi_parameter_preserves_raw_schema_keywords() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let param = RouteParam {
            name: "status".to_string(),
            description: None,
            source: ParamSource::Query,
            ty: "String".to_string(),
            content_media_type: None,
            style: None,
            explode: false,
            allow_reserved: false,
            deprecated: false,
            allow_empty_value: false,
            example: None,
            raw_schema: Some(serde_json::json!({
                "type": "string",
                "if": { "minLength": 1 },
                "then": { "maxLength": 10 }
            })),
            extensions: BTreeMap::new(),
        };
        let route = ParsedRoute {
            path: "/users".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "get_users".to_string(),
            operation_id: None,
            params: vec![param],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let schema = &doc["paths"]["/users"]["get"]["parameters"][0]["schema"];
        assert!(schema.get("if").is_some());
        assert!(schema.get("then").is_some());
    }

    #[test]
    fn test_generate_openapi_request_body_serialized_example() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let route = ParsedRoute {
            path: "/widgets".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "POST".to_string(),
            handler_name: "create_widget".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: Some(crate::openapi::parse::models::RequestBodyDefinition {
                ty: "Widget".into(),
                description: None,
                media_type: "application/json".into(),
                format: crate::openapi::parse::BodyFormat::Json,
                required: true,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
                example: Some(ExampleValue::serialized(serde_json::json!("{\"ok\":true}"))),
            }),
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let media = &doc["paths"]["/widgets"]["post"]["requestBody"]["content"]["application/json"];
        assert!(media.get("example").is_none());
        assert_eq!(
            media["examples"]["example"]["serializedValue"],
            "{\"ok\":true}"
        );
    }

    #[test]
    fn test_generate_openapi_request_body_sequential_item_schema() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let route = ParsedRoute {
            path: "/logs".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "POST".to_string(),
            handler_name: "post_logs".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: Some(RequestBodyDefinition {
                ty: "Vec<String>".into(),
                description: None,
                media_type: "application/x-ndjson".into(),
                format: BodyFormat::Json,
                required: true,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
                example: None,
            }),
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let media =
            &doc["paths"]["/logs"]["post"]["requestBody"]["content"]["application/x-ndjson"];
        assert_eq!(media["itemSchema"]["type"], "string");
        assert_eq!(media["schema"]["type"], "array");
    }

    #[test]
    fn test_generate_openapi_response_serialized_example() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let route = ParsedRoute {
            path: "/status".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "get_status".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: Some("OK".to_string()),
            response_media_type: Some("text/plain".to_string()),
            response_example: Some(ExampleValue::serialized(serde_json::json!("ready"))),
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let media = &doc["paths"]["/status"]["get"]["responses"]["200"]["content"]["text/plain"];
        assert_eq!(media["examples"]["example"]["serializedValue"], "ready");
    }

    #[test]
    fn test_generate_openapi_response_sequential_item_schema() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let route = ParsedRoute {
            path: "/logs".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "get_logs".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: Some("Vec<String>".to_string()),
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: Some("application/x-ndjson".to_string()),
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let media =
            &doc["paths"]["/logs"]["get"]["responses"]["200"]["content"]["application/x-ndjson"];
        assert_eq!(media["itemSchema"]["type"], "string");
        assert_eq!(media["schema"]["type"], "array");
    }

    #[test]
    fn test_generate_openapi_response_metadata() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let route = ParsedRoute {
            path: "/status".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "get_status".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: Some("String".to_string()),
            response_status: Some("201".to_string()),
            response_summary: Some("Created response".to_string()),
            response_description: Some("Created".to_string()),
            response_media_type: Some("text/plain".to_string()),
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        assert_eq!(
            doc["paths"]["/status"]["get"]["responses"]["201"]["summary"],
            "Created response"
        );
        assert_eq!(
            doc["paths"]["/status"]["get"]["responses"]["201"]["description"],
            "Created"
        );
        assert_eq!(
            doc["paths"]["/status"]["get"]["responses"]["201"]["content"]["text/plain"]["schema"]
                ["type"],
            "string"
        );
    }

    #[test]
    fn test_generate_openapi_document_with_routes_security_schemes() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let api_key = SecurityRequirement {
            scheme_name: "ApiKeyAuth".to_string(),
            scopes: vec![],
            scheme: Some(SecuritySchemeInfo {
                kind: SecuritySchemeKind::ApiKey {
                    name: "X-API-Key".to_string(),
                    in_loc: ParamSource::Header,
                },
                description: Some("API key auth".to_string()),
                deprecated: false,
            }),
        };
        let bearer = SecurityRequirement {
            scheme_name: "BearerAuth".to_string(),
            scopes: vec![],
            scheme: Some(SecuritySchemeInfo {
                kind: SecuritySchemeKind::Http {
                    scheme: "bearer".to_string(),
                    bearer_format: Some("JWT".to_string()),
                },
                description: None,
                deprecated: false,
            }),
        };
        let route = ParsedRoute {
            path: "/secure".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "secure".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![crate::openapi::parse::models::SecurityRequirementGroup {
                schemes: vec![api_key, bearer],
            }],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let schemes = &doc["components"]["securitySchemes"];
        assert_eq!(schemes["ApiKeyAuth"]["type"], "apiKey");
        assert_eq!(schemes["ApiKeyAuth"]["name"], "X-API-Key");
        assert_eq!(schemes["ApiKeyAuth"]["in"], "header");
        assert_eq!(schemes["ApiKeyAuth"]["description"], "API key auth");
        assert_eq!(schemes["BearerAuth"]["type"], "http");
        assert_eq!(schemes["BearerAuth"]["scheme"], "bearer");
        assert_eq!(schemes["BearerAuth"]["bearerFormat"], "JWT");
    }

    #[test]
    fn test_generate_openapi_document_with_top_level_security() {
        let mut info = OpenApiInfo::new("Test API", "1.0.0");
        let api_key = SecurityRequirement {
            scheme_name: "ApiKeyAuth".to_string(),
            scopes: vec!["read".to_string()],
            scheme: Some(SecuritySchemeInfo {
                kind: SecuritySchemeKind::ApiKey {
                    name: "X-API-Key".to_string(),
                    in_loc: ParamSource::Header,
                },
                description: None,
                deprecated: false,
            }),
        };
        let group = crate::openapi::parse::models::SecurityRequirementGroup {
            schemes: vec![api_key.clone()],
        };
        info.security = vec![group.clone()];

        let route = ParsedRoute {
            path: "/secure".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "secure".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![group],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        assert_eq!(doc["security"][0]["ApiKeyAuth"][0], "read");
    }

    #[test]
    fn test_generate_openapi_document_operation_security_empty_override() {
        let mut info = OpenApiInfo::new("Test API", "1.0.0");
        let api_key = SecurityRequirement {
            scheme_name: "ApiKeyAuth".to_string(),
            scopes: vec!["read".to_string()],
            scheme: Some(SecuritySchemeInfo {
                kind: SecuritySchemeKind::ApiKey {
                    name: "X-API-Key".to_string(),
                    in_loc: ParamSource::Header,
                },
                description: None,
                deprecated: false,
            }),
        };
        info.security = vec![crate::openapi::parse::models::SecurityRequirementGroup {
            schemes: vec![api_key],
        }];

        let route = ParsedRoute {
            path: "/public".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "public".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: true,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let security = doc["paths"]["/public"]["get"]["security"]
            .as_array()
            .expect("security must be array");
        assert!(security.is_empty());
    }

    #[test]
    fn test_generate_openapi_document_security_scheme_deprecated() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let deprecated_req = SecurityRequirement {
            scheme_name: "LegacyAuth".to_string(),
            scopes: vec![],
            scheme: Some(SecuritySchemeInfo {
                kind: SecuritySchemeKind::ApiKey {
                    name: "X-LEGACY".to_string(),
                    in_loc: ParamSource::Header,
                },
                description: None,
                deprecated: true,
            }),
        };
        let route = ParsedRoute {
            path: "/legacy".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "legacy".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![crate::openapi::parse::models::SecurityRequirementGroup {
                schemes: vec![deprecated_req],
            }],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let schemes = &doc["components"]["securitySchemes"];
        assert_eq!(schemes["LegacyAuth"]["deprecated"], true);
    }

    #[test]
    fn test_generate_openapi_document_with_optional_security_group() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let route = ParsedRoute {
            path: "/public".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "public".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![crate::openapi::parse::models::SecurityRequirementGroup::anonymous()],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let security = doc["paths"]["/public"]["get"]["security"][0]
            .as_object()
            .expect("security entry must be object");
        assert!(security.is_empty());
    }

    #[test]
    fn test_generate_openapi_document_with_routes_oauth_schemes() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let oauth = SecurityRequirement {
            scheme_name: "OAuth".to_string(),
            scopes: vec!["read".to_string()],
            scheme: Some(SecuritySchemeInfo {
                kind: SecuritySchemeKind::OAuth2 {
                    flows: Box::new(crate::openapi::parse::models::OAuthFlows {
                        implicit: None,
                        password: None,
                        client_credentials: None,
                        authorization_code: None,
                        device_authorization: Some(crate::openapi::parse::models::OAuthFlow {
                            authorization_url: None,
                            device_authorization_url: Some(
                                "https://auth.example.com/device".to_string(),
                            ),
                            token_url: Some("https://auth.example.com/token".to_string()),
                            refresh_url: None,
                            scopes: {
                                let mut scopes = std::collections::BTreeMap::new();
                                scopes.insert("read".to_string(), "read data".to_string());
                                scopes
                            },
                        }),
                    }),
                    oauth2_metadata_url: Some(
                        "https://auth.example.com/.well-known/oauth-authorization-server"
                            .to_string(),
                    ),
                },
                description: Some("OAuth2".to_string()),
                deprecated: false,
            }),
        };
        let oidc = SecurityRequirement {
            scheme_name: "Oidc".to_string(),
            scopes: vec![],
            scheme: Some(SecuritySchemeInfo {
                kind: SecuritySchemeKind::OpenIdConnect {
                    open_id_connect_url:
                        "https://auth.example.com/.well-known/openid-configuration".to_string(),
                },
                description: None,
                deprecated: false,
            }),
        };
        let route = ParsedRoute {
            path: "/secure".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "secure".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![crate::openapi::parse::models::SecurityRequirementGroup {
                schemes: vec![oauth, oidc],
            }],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let schemes = &doc["components"]["securitySchemes"];
        assert_eq!(schemes["OAuth"]["type"], "oauth2");
        assert_eq!(
            schemes["OAuth"]["oauth2MetadataUrl"],
            "https://auth.example.com/.well-known/oauth-authorization-server"
        );
        assert_eq!(
            schemes["OAuth"]["flows"]["deviceAuthorization"]["deviceAuthorizationUrl"],
            "https://auth.example.com/device"
        );
        assert_eq!(
            schemes["OAuth"]["flows"]["deviceAuthorization"]["tokenUrl"],
            "https://auth.example.com/token"
        );
        assert_eq!(
            schemes["OAuth"]["flows"]["deviceAuthorization"]["scopes"]["read"],
            "read data"
        );
        assert_eq!(schemes["Oidc"]["type"], "openIdConnect");
        assert_eq!(
            schemes["Oidc"]["openIdConnectUrl"],
            "https://auth.example.com/.well-known/openid-configuration"
        );
    }

    #[test]
    fn test_generate_openapi_document_security_scheme_conflict() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let route_a = ParsedRoute {
            path: "/a".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "a".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![crate::openapi::parse::models::SecurityRequirementGroup {
                schemes: vec![SecurityRequirement {
                    scheme_name: "ApiKeyAuth".to_string(),
                    scopes: vec![],
                    scheme: Some(SecuritySchemeInfo {
                        kind: SecuritySchemeKind::ApiKey {
                            name: "X-ONE".to_string(),
                            in_loc: ParamSource::Header,
                        },
                        description: None,
                        deprecated: false,
                    }),
                }],
            }],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };
        let route_b = ParsedRoute {
            path: "/b".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "b".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![crate::openapi::parse::models::SecurityRequirementGroup {
                schemes: vec![SecurityRequirement {
                    scheme_name: "ApiKeyAuth".to_string(),
                    scopes: vec![],
                    scheme: Some(SecuritySchemeInfo {
                        kind: SecuritySchemeKind::ApiKey {
                            name: "X-TWO".to_string(),
                            in_loc: ParamSource::Header,
                        },
                        description: None,
                        deprecated: false,
                    }),
                }],
            }],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let err = generate_openapi_document_with_routes(&[], &[route_a, route_b], None, &info)
            .expect_err("expected error");
        assert!(format!("{err}").contains("Conflicting security scheme definitions"));
    }

    #[test]
    fn test_generate_openapi_document_with_routes_top_level_servers() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let base_path = Some("/api/v1".to_string());
        let route_a = ParsedRoute {
            path: "/users".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: base_path.clone(),

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "list_users".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };
        let route_b = ParsedRoute {
            path: "/groups".to_string(),
            summary: None,
            description: None,
            path_summary: None,
            path_description: None,
            path_extensions: BTreeMap::new(),
            operation_summary: None,
            operation_description: None,
            base_path: base_path.clone(),
            path_servers: None,
            servers_override: None,
            method: "GET".to_string(),
            handler_name: "list_groups".to_string(),
            operation_id: None,
            params: vec![],
            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route_a, route_b], None, &info)
            .expect("Failed to generate openapi document with routes");
        assert_eq!(doc["servers"][0]["url"], "/api/v1");
        assert!(doc["paths"]["/users"]["get"].get("servers").is_none());
        assert!(doc["paths"]["/groups"]["get"].get("servers").is_none());
    }

    #[test]
    fn test_generate_openapi_document_with_routes_operation_servers() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let route_a = ParsedRoute {
            path: "/users".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: Some("/api/v1".to_string()),

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "list_users".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };
        let route_b = ParsedRoute {
            path: "/groups".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: Some("/api/v2".to_string()),

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "list_groups".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route_a, route_b], None, &info)
            .expect("Failed to generate openapi document with routes");
        assert!(doc.get("servers").is_none());
        assert_eq!(
            doc["paths"]["/users"]["get"]["servers"][0]["url"],
            "/api/v1"
        );
        assert_eq!(
            doc["paths"]["/groups"]["get"]["servers"][0]["url"],
            "/api/v2"
        );
    }

    #[test]
    fn test_generate_openapi_document_with_route_server_override() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let mut variables = BTreeMap::new();
        variables.insert(
            "env".to_string(),
            ParsedServerVariable {
                enum_values: Some(vec!["prod".to_string()]),
                default: "prod".to_string(),
                description: Some("environment".to_string()),
            },
        );

        let route = ParsedRoute {
            path: "/users".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: Some("/v1".to_string()),

            path_servers: None,

            servers_override: Some(vec![ParsedServer {
                url: "https://{env}.example.com/v1".to_string(),
                description: Some("override".to_string()),
                name: Some("override".to_string()),
                variables,
            }]),
            method: "GET".to_string(),
            handler_name: "list_users".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        assert!(doc.get("servers").is_none());
        let server = &doc["paths"]["/users"]["get"]["servers"][0];
        assert_eq!(server["url"], "https://{env}.example.com/v1");
        assert_eq!(server["name"], "override");
        assert_eq!(server["description"], "override");
        assert_eq!(server["variables"]["env"]["default"], "prod");
    }

    #[test]
    fn test_generate_openapi_document_with_routes_webhooks() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let route = ParsedRoute {
            path: "onEvent".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "POST".to_string(),
            handler_name: "on_event".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Webhook,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        assert!(doc["webhooks"]["onEvent"]["post"].is_object());
    }

    #[test]
    fn test_generate_openapi_document_emits_paths_and_webhooks_extensions() {
        let info = OpenApiInfo::new("Test API", "1.0.0")
            .with_paths_extension("x-paths-meta", json!({"owner": "api"}))
            .with_webhooks_extension("x-webhooks-meta", json!(true));

        let webhook_route = ParsedRoute {
            path: "onEvent".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "POST".to_string(),
            handler_name: "on_event".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Webhook,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[webhook_route], None, &info)
            .expect("Failed to generate openapi document with routes");
        assert_eq!(doc["paths"]["x-paths-meta"]["owner"], "api");
        assert_eq!(doc["webhooks"]["x-webhooks-meta"], true);
    }

    #[test]
    fn test_generate_openapi_document_with_routes_custom_method_additional_operations() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let route = ParsedRoute {
            path: "/files".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "COPY".to_string(),
            handler_name: "copy_file".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        assert!(doc["paths"]["/files"]["additionalOperations"]["COPY"].is_object());
    }

    #[test]
    fn test_generate_openapi_document_with_callbacks() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let path_param = RouteParam {
            name: "limit".to_string(),
            description: None,
            source: ParamSource::Query,
            ty: "i32".to_string(),
            content_media_type: None,
            style: None,
            explode: true,
            allow_reserved: false,
            deprecated: false,
            allow_empty_value: false,
            example: None,
            raw_schema: None,
            extensions: BTreeMap::new(),
        };
        let op_param = RouteParam {
            name: "offset".to_string(),
            description: None,
            source: ParamSource::Query,
            ty: "i32".to_string(),
            content_media_type: None,
            style: None,
            explode: true,
            allow_reserved: false,
            deprecated: false,
            allow_empty_value: false,
            example: None,
            raw_schema: None,
            extensions: BTreeMap::new(),
        };
        let callback_body = RequestBodyDefinition {
            ty: "CallbackPayload".to_string(),
            description: None,
            media_type: "application/json".to_string(),
            format: BodyFormat::Json,
            required: true,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
            example: None,
        };
        let callbacks = vec![
            ParsedCallback {
                name: "onData".to_string(),
                expression: RuntimeExpression::new("$request.body#/url"),
                method: "POST".to_string(),
                params: vec![op_param.clone()],
                path_params: vec![path_param.clone()],
                security: vec![],
                security_defined: false,
                request_body: Some(callback_body.clone()),
                response_type: Some("String".to_string()),
                response_status: Some("202".to_string()),
                response_summary: None,
                response_description: Some("Accepted".to_string()),
                response_media_type: Some("text/plain".to_string()),
                response_example: None,
                response_headers: vec![ResponseHeader {
                    name: "X-Callback".to_string(),
                    description: None,
                    required: false,
                    deprecated: false,
                    style: None,
                    explode: None,
                    ty: "String".to_string(),
                    content_media_type: None,
                    example: None,
                    extensions: BTreeMap::new(),
                }],
            },
            ParsedCallback {
                name: "onData".to_string(),
                expression: RuntimeExpression::new("$request.body#/url"),
                method: "COPY".to_string(),
                params: vec![op_param],
                path_params: vec![path_param],
                security: vec![],
                security_defined: false,
                request_body: Some(callback_body),
                response_type: None,
                response_status: None,
                response_summary: None,
                response_description: None,
                response_media_type: None,
                response_example: None,
                response_headers: vec![],
            },
        ];

        let route = ParsedRoute {
            path: "/subscribe".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "POST".to_string(),
            handler_name: "subscribe".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks,
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let callbacks =
            &doc["paths"]["/subscribe"]["post"]["callbacks"]["onData"]["$request.body#/url"];
        assert!(callbacks["post"].is_object());
        assert!(callbacks["parameters"].is_array());
        assert!(callbacks["post"]["parameters"].is_array());
        assert!(callbacks["additionalOperations"]["COPY"].is_object());
        assert!(callbacks["post"]["requestBody"].is_object());
        assert_eq!(
            callbacks["post"]["responses"]["202"]["description"],
            "Accepted"
        );
        assert_eq!(
            callbacks["post"]["responses"]["202"]["content"]["text/plain"]["schema"]["type"],
            "string"
        );
    }

    #[test]
    fn test_generate_openapi_document_with_callback_security() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let security_req = SecurityRequirement {
            scheme_name: "ApiKeyAuth".to_string(),
            scopes: vec![],
            scheme: Some(SecuritySchemeInfo {
                kind: SecuritySchemeKind::ApiKey {
                    name: "X-API-Key".to_string(),
                    in_loc: ParamSource::Header,
                },
                description: None,
                deprecated: false,
            }),
        };
        let callbacks = vec![ParsedCallback {
            name: "onData".to_string(),
            expression: RuntimeExpression::new("$request.body#/url"),
            method: "POST".to_string(),
            params: vec![],
            path_params: vec![],
            security: vec![crate::openapi::parse::models::SecurityRequirementGroup {
                schemes: vec![security_req],
            }],
            security_defined: true,
            request_body: None,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
        }];

        let route = ParsedRoute {
            path: "/subscribe".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "POST".to_string(),
            handler_name: "subscribe".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks,
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let cb_security = &doc["paths"]["/subscribe"]["post"]["callbacks"]["onData"]
            ["$request.body#/url"]["post"]["security"];
        assert!(cb_security.is_array());
        assert!(doc["components"]["securitySchemes"]["ApiKeyAuth"].is_object());
    }

    #[test]
    fn test_generate_openapi_document_merges_raw_responses() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let raw_responses = json!({
            "200": {
                "description": "raw",
                "content": {
                    "text/plain": { "schema": { "type": "string" } }
                }
            },
            "404": { "description": "Not Found" }
        });

        let route = ParsedRoute {
            path: "/items".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "get_items".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: Some("i32".to_string()),
            response_status: Some("200".to_string()),
            response_summary: None,
            response_description: Some("OK".to_string()),
            response_media_type: Some("application/json".to_string()),
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: Some(raw_responses),
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let responses = &doc["paths"]["/items"]["get"]["responses"];
        assert_eq!(responses["404"]["description"], "Not Found");
        assert_eq!(
            responses["200"]["content"]["application/json"]["schema"]["type"],
            "integer"
        );
    }

    #[test]
    fn test_generate_openapi_document_preserves_header_content() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let raw_responses = json!({
            "200": {
                "description": "OK",
                "headers": {
                    "X-Token": {
                        "content": {
                            "text/plain": { "schema": { "type": "string" } }
                        }
                    }
                }
            }
        });

        let route = ParsedRoute {
            path: "/items".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "get_items".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: Some(raw_responses),
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let header = &doc["paths"]["/items"]["get"]["responses"]["200"]["headers"]["X-Token"];
        assert!(header.get("content").is_some());
    }

    #[test]
    fn test_generate_openapi_document_merges_raw_request_body() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let raw_request_body = json!({
            "description": "raw",
            "content": {
                "text/plain": { "schema": { "type": "string" } },
                "application/json": { "schema": { "type": "object" } }
            }
        });

        let route = ParsedRoute {
            path: "/items".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "POST".to_string(),
            handler_name: "create_item".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: Some(RequestBodyDefinition {
                ty: "String".to_string(),
                description: Some("updated".to_string()),
                media_type: "application/json".to_string(),
                format: BodyFormat::Json,
                required: true,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
                example: None,
            }),
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: Some("200".to_string()),
            response_summary: None,
            response_description: Some("OK".to_string()),
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: Some(raw_request_body),
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let request_body = &doc["paths"]["/items"]["post"]["requestBody"];
        assert_eq!(request_body["description"], "updated");
        assert_eq!(request_body["required"], true);
        assert_eq!(
            request_body["content"]["application/json"]["schema"]["type"],
            "string"
        );
        assert_eq!(
            request_body["content"]["text/plain"]["schema"]["type"],
            "string"
        );
    }

    #[test]
    fn test_generate_openapi_document_merges_raw_request_body_schema_keywords() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let raw_schema = json!({
            "type": "object",
            "properties": {
                "payload": {
                    "type": "string",
                    "contentSchema": {
                        "type": "object",
                        "properties": { "id": { "type": "string" } }
                    }
                }
            },
            "if": { "properties": { "kind": { "const": "a" } } },
            "then": { "required": ["payload"] },
            "else": { "not": { "required": ["payload"] } },
            "dependentSchemas": {
                "payload": { "properties": { "id": { "type": "string" } } }
            },
            "unevaluatedProperties": false
        });
        let raw_request_body = json!({
            "content": {
                "application/json": { "schema": raw_schema }
            }
        });

        let route = ParsedRoute {
            path: "/items".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "POST".to_string(),
            handler_name: "create_item".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: Some(RequestBodyDefinition {
                ty: "HashMap<String, String>".to_string(),
                description: None,
                media_type: "application/json".to_string(),
                format: BodyFormat::Json,
                required: true,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
                example: None,
            }),
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: Some("200".to_string()),
            response_summary: None,
            response_description: Some("OK".to_string()),
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: Some(raw_request_body),
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let schema =
            &doc["paths"]["/items"]["post"]["requestBody"]["content"]["application/json"]["schema"];
        assert!(schema.get("if").is_some());
        assert!(schema.get("else").is_some());
        assert!(schema.get("dependentSchemas").is_some());
        assert_eq!(schema["unevaluatedProperties"], false);
        assert!(schema["properties"]["payload"]["contentSchema"].is_object());
    }

    #[test]
    fn test_generate_openapi_document_merges_raw_response_schema_keywords() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let raw_schema = json!({
            "type": "object",
            "properties": {
                "payload": {
                    "type": "string",
                    "contentSchema": {
                        "type": "object",
                        "properties": { "id": { "type": "string" } }
                    }
                }
            },
            "if": { "properties": { "kind": { "const": "a" } } },
            "then": { "required": ["payload"] },
            "else": { "not": { "required": ["payload"] } },
            "dependentSchemas": {
                "payload": { "properties": { "id": { "type": "string" } } }
            },
            "unevaluatedProperties": false
        });
        let raw_responses = json!({
            "200": {
                "description": "raw",
                "content": {
                    "application/json": { "schema": raw_schema }
                }
            }
        });

        let route = ParsedRoute {
            path: "/items".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "GET".to_string(),
            handler_name: "get_items".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: Some("HashMap<String, String>".to_string()),
            response_status: Some("200".to_string()),
            response_summary: None,
            response_description: Some("OK".to_string()),
            response_media_type: Some("application/json".to_string()),
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: Some(raw_responses),
            extensions: BTreeMap::new(),
        };

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info)
            .expect("Failed to generate openapi document with routes");
        let schema = &doc["paths"]["/items"]["get"]["responses"]["200"]["content"]
            ["application/json"]["schema"];
        assert!(schema.get("if").is_some());
        assert!(schema.get("else").is_some());
        assert!(schema.get("dependentSchemas").is_some());
        assert_eq!(schema["unevaluatedProperties"], false);
        assert!(schema["properties"]["payload"]["contentSchema"].is_object());
    }

    #[test]
    fn test_generate_openapi_document_emits_nested_encoding() {
        use std::collections::HashMap;

        let info = OpenApiInfo::new("Test API", "1.0.0");
        let mut nested = HashMap::new();
        nested.insert(
            "part".to_string(),
            EncodingInfo {
                content_type: Some("application/json".to_string()),
                headers: HashMap::new(),
                style: None,
                explode: None,
                allow_reserved: None,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
            },
        );

        let mut encoding = HashMap::new();
        encoding.insert(
            "payload".to_string(),
            EncodingInfo {
                content_type: Some("multipart/mixed".to_string()),
                headers: HashMap::new(),
                style: None,
                explode: None,
                allow_reserved: None,
                encoding: Some(nested),
                prefix_encoding: Some(vec![EncodingInfo {
                    content_type: Some("text/plain".to_string()),
                    headers: HashMap::new(),
                    style: None,
                    explode: None,
                    allow_reserved: None,
                    encoding: None,
                    prefix_encoding: None,
                    item_encoding: None,
                }]),
                item_encoding: Some(Box::new(EncodingInfo {
                    content_type: Some("application/octet-stream".to_string()),
                    headers: HashMap::new(),
                    style: None,
                    explode: None,
                    allow_reserved: None,
                    encoding: None,
                    prefix_encoding: None,
                    item_encoding: None,
                })),
            },
        );

        let route = ParsedRoute {
            path: "/upload".to_string(),
            summary: None,
            description: None,

            path_summary: None,

            path_description: None,
            path_extensions: BTreeMap::new(),

            operation_summary: None,

            operation_description: None,

            base_path: None,

            path_servers: None,

            servers_override: None,
            method: "POST".to_string(),
            handler_name: "upload".to_string(),
            operation_id: None,
            params: vec![],

            path_params: vec![],

            request_body: Some(RequestBodyDefinition {
                ty: "Upload".to_string(),
                description: None,
                media_type: "multipart/form-data".to_string(),
                format: BodyFormat::Multipart,
                required: true,
                encoding: Some(encoding),
                prefix_encoding: None,
                item_encoding: None,
                example: None,
            }),
            security: vec![],
            security_defined: false,
            response_type: None,
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        };

        let doc =
            generate_openapi_document_with_routes(&[], std::slice::from_ref(&route), None, &info)
                .expect("Failed to generate openapi document with routes");
        let enc = &doc["paths"]["/upload"]["post"]["requestBody"]["content"]["multipart/form-data"]
            ["encoding"]["payload"];
        assert_eq!(enc["contentType"], "multipart/mixed");
        assert_eq!(enc["encoding"]["part"]["contentType"], "application/json");
        assert_eq!(enc["prefixEncoding"][0]["contentType"], "text/plain");
        assert_eq!(
            enc["itemEncoding"]["contentType"],
            "application/octet-stream"
        );

        // Comprehensive tests for remaining branches in openapi/emit.rs:
        // 1. All builder methods on OpenApiInfo, OpenApiContact, OpenApiLicense, OpenApiTag, OpenApiServer, OpenApiServerVariable
        let mut ext_map = BTreeMap::new();
        ext_map.insert("x-root".to_string(), json!(1));
        ext_map.insert("ignored_non_x".to_string(), json!(2));
        let mut path_ext_map = BTreeMap::new();
        path_ext_map.insert("x-path".to_string(), json!(true));
        let mut wh_ext_map = BTreeMap::new();
        wh_ext_map.insert("x-wh".to_string(), json!("v"));

        let mut vars_map = BTreeMap::new();
        vars_map.insert(
            "v1".to_string(),
            OpenApiServerVariable::new("def")
                .with_enum_values(vec!["def".to_string()])
                .with_description("var desc"),
        );
        let srv1 = OpenApiServer::new("https://s1.example.com")
            .with_description("s1 desc")
            .with_name("S1")
            .with_variables(vars_map)
            .with_variable("v2", OpenApiServerVariable::new("d2"));

        let tag1 = OpenApiTag::new("t1")
            .with_summary("sum")
            .with_description("desc")
            .with_external_docs("https://example.com/t1", Some("tdesc".to_string()))
            .with_parent("p1")
            .with_kind("nav");

        let contact_empty = OpenApiContact::default();
        let contact_full = OpenApiContact::new()
            .with_name("Alice")
            .with_url("https://example.com")
            .with_email("a@example.com");
        let lic_id = OpenApiLicense::new("MIT").with_identifier("MIT");
        let lic_url = OpenApiLicense::new("MIT").with_url("https://example.com/mit");
        let lic_both = OpenApiLicense::new("MIT")
            .with_identifier("MIT")
            .with_url("https://example.com/mit");

        let sec_group_empty =
            crate::openapi::parse::models::SecurityRequirementGroup { schemes: vec![] };
        let sec_group_oauth = crate::openapi::parse::models::SecurityRequirementGroup {
            schemes: vec![
                SecurityRequirement {
                    scheme_name: "OAuthScheme".to_string(),
                    scopes: vec!["read".to_string()],
                    scheme: Some(SecuritySchemeInfo {
                        kind: SecuritySchemeKind::OAuth2 {
                            flows: Box::new(crate::openapi::parse::models::OAuthFlows {
                                implicit: Some(crate::openapi::parse::models::OAuthFlow {
                                    authorization_url: Some("https://example.com/auth".to_string()),
                                    token_url: None,
                                    refresh_url: Some("https://example.com/refresh".to_string()),
                                    device_authorization_url: None,
                                    scopes: BTreeMap::new(),
                                }),
                                password: Some(crate::openapi::parse::models::OAuthFlow {
                                    authorization_url: None,
                                    token_url: Some("https://example.com/token".to_string()),
                                    refresh_url: None,
                                    device_authorization_url: None,
                                    scopes: BTreeMap::new(),
                                }),
                                client_credentials: Some(
                                    crate::openapi::parse::models::OAuthFlow {
                                        authorization_url: None,
                                        token_url: Some("https://example.com/token".to_string()),
                                        refresh_url: None,
                                        device_authorization_url: None,
                                        scopes: BTreeMap::new(),
                                    },
                                ),
                                authorization_code: Some(
                                    crate::openapi::parse::models::OAuthFlow {
                                        authorization_url: Some(
                                            "https://example.com/auth".to_string(),
                                        ),
                                        token_url: Some("https://example.com/token".to_string()),
                                        refresh_url: None,
                                        device_authorization_url: None,
                                        scopes: BTreeMap::new(),
                                    },
                                ),
                                device_authorization: Some(
                                    crate::openapi::parse::models::OAuthFlow {
                                        authorization_url: None,
                                        token_url: Some("https://example.com/token".to_string()),
                                        refresh_url: None,
                                        device_authorization_url: Some(
                                            "https://example.com/device".to_string(),
                                        ),
                                        scopes: BTreeMap::new(),
                                    },
                                ),
                            }),
                            oauth2_metadata_url: Some("https://example.com/meta".to_string()),
                        },
                        description: Some("OAuth desc".to_string()),
                        deprecated: true,
                    }),
                },
                SecurityRequirement {
                    scheme_name: "OAuthBareScheme".to_string(),
                    scopes: vec![],
                    scheme: Some(SecuritySchemeInfo {
                        kind: SecuritySchemeKind::OAuth2 {
                            flows: Box::new(crate::openapi::parse::models::OAuthFlows {
                                implicit: None,
                                password: None,
                                client_credentials: None,
                                authorization_code: None,
                                device_authorization: None,
                            }),
                            oauth2_metadata_url: None,
                        },
                        description: None,
                        deprecated: false,
                    }),
                },
                SecurityRequirement {
                    scheme_name: "OidcScheme".to_string(),
                    scopes: vec![],
                    scheme: Some(SecuritySchemeInfo {
                        kind: SecuritySchemeKind::OpenIdConnect {
                            open_id_connect_url: "https://example.com/oidc".to_string(),
                        },
                        description: None,
                        deprecated: false,
                    }),
                },
                SecurityRequirement {
                    scheme_name: "MtlsScheme".to_string(),
                    scopes: vec![],
                    scheme: Some(SecuritySchemeInfo {
                        kind: SecuritySchemeKind::MutualTls,
                        description: None,
                        deprecated: false,
                    }),
                },
                SecurityRequirement {
                    scheme_name: "HttpBearer".to_string(),
                    scopes: vec![],
                    scheme: Some(SecuritySchemeInfo {
                        kind: SecuritySchemeKind::Http {
                            scheme: "bearer".to_string(),
                            bearer_format: Some("JWT".to_string()),
                        },
                        description: None,
                        deprecated: false,
                    }),
                },
                SecurityRequirement {
                    scheme_name: "HttpBasic".to_string(),
                    scopes: vec![],
                    scheme: Some(SecuritySchemeInfo {
                        kind: SecuritySchemeKind::Http {
                            scheme: "basic".to_string(),
                            bearer_format: None,
                        },
                        description: None,
                        deprecated: false,
                    }),
                },
                SecurityRequirement {
                    scheme_name: "ApiKeyQ".to_string(),
                    scopes: vec![],
                    scheme: Some(SecuritySchemeInfo {
                        kind: SecuritySchemeKind::ApiKey {
                            name: "q".to_string(),
                            in_loc: ParamSource::Query,
                        },
                        description: None,
                        deprecated: false,
                    }),
                },
                SecurityRequirement {
                    scheme_name: "ApiKeyC".to_string(),
                    scopes: vec![],
                    scheme: Some(SecuritySchemeInfo {
                        kind: SecuritySchemeKind::ApiKey {
                            name: "c".to_string(),
                            in_loc: ParamSource::Cookie,
                        },
                        description: None,
                        deprecated: false,
                    }),
                },
                SecurityRequirement {
                    scheme_name: "ApiKeyInvalidLoc".to_string(),
                    scopes: vec![],
                    scheme: Some(SecuritySchemeInfo {
                        kind: SecuritySchemeKind::ApiKey {
                            name: "p".to_string(),
                            in_loc: ParamSource::Path,
                        },
                        description: None,
                        deprecated: false,
                    }),
                },
                SecurityRequirement {
                    scheme_name: "UnresolvedScheme".to_string(),
                    scopes: vec![],
                    scheme: None,
                },
            ],
        };

        let rich_info = OpenApiInfo::new("Rich", "1.0")
            .with_summary("Sum")
            .with_description("Desc")
            .with_terms_of_service("https://example.com/tos")
            .with_contact(contact_full)
            .with_license(lic_id)
            .with_self_uri("https://example.com/openapi.json")
            .with_external_docs("https://example.com/docs", Some("ExtDesc".to_string()))
            .with_servers(vec![srv1.clone()])
            .with_server(OpenApiServer::new("https://s2.example.com"))
            .with_tags(vec![tag1.clone(), tag1.clone()])
            .with_tag(OpenApiTag::new("t2"))
            .with_security(vec![sec_group_empty.clone(), sec_group_oauth.clone()])
            .with_paths_extensions(path_ext_map)
            .with_paths_extension("x-p2", json!(2))
            .with_webhooks_extensions(wh_ext_map)
            .with_webhooks_extension("x-w2", json!(3))
            .with_extensions(ext_map)
            .with_extension("x-r2", json!(4));

        let simple_model = ParsedModel::Struct(make_struct(
            "Item",
            vec![make_field("id", "Option<Vec<u8>>", None)],
        ));
        let doc1 = generate_openapi_document(
            &simple_model,
            Some("https://spec.openapis.org/oas/3.1/dialect/base"),
            &rich_info,
        )
        .expect("doc1");
        assert!(doc1.get("$self").is_some());

        let info_empty_contact_url_lic = OpenApiInfo::new("T", "1.0")
            .with_contact(contact_empty)
            .with_license(lic_url);
        let _ = generate_openapi_document(&simple_model, None, &info_empty_contact_url_lic)
            .expect("doc2");

        let info_bad_lic = OpenApiInfo::new("T", "1.0").with_license(lic_both);
        assert!(generate_openapi_document(&simple_model, None, &info_bad_lic).is_err());

        let info_bad_path_ext =
            OpenApiInfo::new("T", "1.0").with_paths_extension("bad_key", json!(1));
        assert!(generate_openapi_document(&simple_model, None, &info_bad_path_ext).is_err());

        // Empty document error (no models, no routes)
        assert!(
            generate_openapi_document_with_routes(&[], &[], None, &OpenApiInfo::new("E", "1"))
                .is_err()
        );

        // 2. Routes withwebhooks, custom methods, path extensions (matching & conflicting), path_servers,
        //servers_override, base_path, raw_request_body (with & without request_body, non-object raw),
        // raw_responses, callbacks (with custom method, params, request_body, security, headers),
        // links (with server_url vs server, literal vs expression), examples (Data with meta, Serialized non-string, External string & non-string),
        // all ParamSources & ParamStyles, sequential response/request itemSchema (including Option<Vec<T>>)
        let mut r1 = route.clone();
        r1.path = "/custom/{id}".to_string();
        r1.method = "CUSTOM".to_string();
        r1.path_summary = Some("PSum".to_string());
        r1.path_description = Some("PDesc".to_string());
        r1.summary = Some("IgnoredFallbackSum".to_string());
        r1.description = Some("IgnoredFallbackDesc".to_string());
        r1.deprecated = true;
        r1.tags = vec!["t1".to_string(), "new_tag".to_string(), "  ".to_string()];
        r1.external_docs = Some(ParsedExternalDocs {
            url: "https://example.com/op".to_string(),
            description: None,
        });
        r1.path_extensions.insert("x-pe".to_string(), json!("same"));
        let mut parsed_srv_vars = BTreeMap::new();
        parsed_srv_vars.insert(
            "v".to_string(),
            ParsedServerVariable {
                enum_values: Some(vec!["a".to_string()]),
                default: "a".to_string(),
                description: Some("vd".to_string()),
            },
        );
        parsed_srv_vars.insert(
            "v_bare".to_string(),
            ParsedServerVariable {
                enum_values: None,
                default: "b".to_string(),
                description: None,
            },
        );
        r1.path_servers = Some(vec![ParsedServer {
            url: "https://ps.example.com/{v}".to_string(),
            description: Some("psd".to_string()),
            name: Some("PS".to_string()),
            variables: parsed_srv_vars,
        }]);
        r1.servers_override = Some(vec![ParsedServer {
            url: "https://ovr.example.com".to_string(),
            description: None,
            name: None,
            variables: BTreeMap::new(),
        }]);
        let path_param = RouteParam {
            name: "id".to_string(),
            source: ParamSource::Path,
            ty: "String".to_string(),
            description: Some("id desc".to_string()),
            deprecated: true,
            allow_empty_value: true,
            allow_reserved: true,
            style: Some(ParamStyle::Simple),
            explode: false,
            example: Some(ExampleValue::data_with_meta(
                json!("123"),
                Some("sum".to_string()),
                Some("desc".to_string()),
            )),
            content_media_type: None,
            raw_schema: Some(json!({ "minLength": 1 })),
            extensions: BTreeMap::new(),
        };
        r1.path_params = vec![path_param.clone()];
        r1.params = vec![
            path_param,
            RouteParam {
                name: "qs".to_string(),
                source: ParamSource::QueryString,
                ty: "serde_json::Value".to_string(),
                description: None,
                deprecated: false,
                allow_empty_value: false,
                allow_reserved: false,
                style: None,
                explode: true,
                example: Some(ExampleValue::serialized(json!(123))),
                content_media_type: None,
                raw_schema: Some(json!("not_an_obj")),
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "hdr".to_string(),
                source: ParamSource::Header,
                ty: "Option<String>".to_string(),
                description: None,
                deprecated: false,
                allow_empty_value: false,
                allow_reserved: false,
                style: Some(ParamStyle::Matrix),
                explode: false,
                example: Some(ExampleValue::external(json!("https://example.com/ex"))),
                content_media_type: None,
                raw_schema: None,
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "ck".to_string(),
                source: ParamSource::Cookie,
                ty: "String".to_string(),
                description: None,
                deprecated: false,
                allow_empty_value: false,
                allow_reserved: false,
                style: Some(ParamStyle::Cookie),
                explode: true,
                example: Some(ExampleValue::external(json!(456))),
                content_media_type: None,
                raw_schema: None,
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "fd".to_string(),
                source: ParamSource::FormData,
                ty: "String".to_string(),
                description: None,
                deprecated: false,
                allow_empty_value: false,
                allow_reserved: false,
                style: Some(ParamStyle::Label),
                explode: false,
                example: None,
                content_media_type: None,
                raw_schema: None,
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "bd".to_string(),
                source: ParamSource::Body,
                ty: "String".to_string(),
                description: None,
                deprecated: false,
                allow_empty_value: false,
                allow_reserved: false,
                style: Some(ParamStyle::SpaceDelimited),
                explode: false,
                example: None,
                content_media_type: None,
                raw_schema: None,
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "q2".to_string(),
                source: ParamSource::Query,
                ty: "String".to_string(),
                description: None,
                deprecated: false,
                allow_empty_value: false,
                allow_reserved: false,
                style: Some(ParamStyle::PipeDelimited),
                explode: false,
                example: None,
                content_media_type: None,
                raw_schema: None,
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "q3".to_string(),
                source: ParamSource::Query,
                ty: "Vec<u8>".to_string(),
                description: None,
                deprecated: false,
                allow_empty_value: false,
                allow_reserved: false,
                style: Some(ParamStyle::DeepObject),
                explode: true,
                example: None,
                content_media_type: Some(crate::openapi::parse::models::ContentMediaType::Json),
                raw_schema: Some(json!({ "type": 123 })),
                extensions: BTreeMap::new(),
            },
            RouteParam {
                name: "q4".to_string(),
                source: ParamSource::Query,
                ty: "String".to_string(),
                description: None,
                deprecated: false,
                allow_empty_value: false,
                allow_reserved: false,
                style: Some(ParamStyle::DeepObject),
                explode: true,
                example: None,
                content_media_type: None,
                raw_schema: None,
                extensions: BTreeMap::new(),
            },
        ];
        r1.security = vec![sec_group_oauth.clone()];
        r1.security_defined = true;
        let mut enc_hdrs = std::collections::HashMap::new();
        enc_hdrs.insert("X-Part".to_string(), "String".to_string());
        let rich_enc_info = EncodingInfo {
            content_type: Some("text/plain".to_string()),
            headers: enc_hdrs,
            style: Some(ParamStyle::Form),
            explode: Some(true),
            allow_reserved: Some(true),
            encoding: Some(std::collections::HashMap::new()),
            prefix_encoding: Some(vec![]),
            item_encoding: None,
        };
        let bare_enc_info = EncodingInfo {
            content_type: None,
            headers: std::collections::HashMap::new(),
            style: None,
            explode: None,
            allow_reserved: None,
            encoding: None,
            prefix_encoding: None,
            item_encoding: None,
        };
        r1.request_body = Some(RequestBodyDefinition {
            ty: "Option<Vec<String>>".to_string(),
            description: Some("RB Desc".to_string()),
            media_type: "application/jsonl".to_string(),
            format: BodyFormat::Json,
            required: true,
            encoding: Some(std::collections::HashMap::new()),
            prefix_encoding: Some(vec![rich_enc_info.clone(), bare_enc_info]),
            item_encoding: Some(rich_enc_info),
            example: Some(ExampleValue::data(json!("ex"))),
        });
        r1.response_summary = Some("RSum".to_string());
        r1.response_type = Some("Option<Vec<String>>".to_string());
        r1.response_media_type = Some("application/jsonl".to_string());
        r1.response_example = Some(ExampleValue::data(json!("rex")));
        r1.response_headers = vec![
            ResponseHeader {
                name: "X-H1".to_string(),
                description: None,
                required: false,
                deprecated: false,
                style: None,
                explode: None,
                ty: "String".to_string(),
                content_media_type: Some("text/plain".to_string()),
                example: None,
                extensions: BTreeMap::new(),
            },
            ResponseHeader {
                name: "X-H2".to_string(),
                description: None,
                required: false,
                deprecated: false,
                style: None,
                explode: None,
                ty: "String".to_string(),
                content_media_type: None,
                example: Some(ExampleValue::data(json!("h2"))),
                extensions: BTreeMap::new(),
            },
        ];
        let mut link_params = std::collections::HashMap::new();
        link_params.insert(
            "p1".to_string(),
            LinkParamValue::Expression(RuntimeExpression::new("$request.path.id")),
        );
        link_params.insert("p2".to_string(), LinkParamValue::Literal(json!("v2")));
        r1.response_links = Some(vec![
            ParsedLink {
                name: "L1".to_string(),
                description: Some("ldesc".to_string()),
                operation_id: Some("op1".to_string()),
                operation_ref: Some("/custom/1".to_string()),
                resolved_operation_ref: None,
                parameters: link_params,
                request_body: Some(LinkRequestBody::Expression(RuntimeExpression::new(
                    "$request.body",
                ))),
                server: Some(ParsedServer {
                    url: "https://ls.example.com".to_string(),
                    description: None,
                    name: None,
                    variables: BTreeMap::new(),
                }),
                server_url: None,
            },
            ParsedLink {
                name: "L2".to_string(),
                description: None,
                operation_id: None,
                operation_ref: None,
                resolved_operation_ref: None,
                parameters: std::collections::HashMap::new(),
                request_body: Some(LinkRequestBody::Literal(json!({"a": 1}))),
                server: None,
                server_url: Some("https://ls2.example.com".to_string()),
            },
            ParsedLink {
                name: "L3".to_string(),
                description: None,
                operation_id: None,
                operation_ref: None,
                resolved_operation_ref: None,
                parameters: std::collections::HashMap::new(),
                request_body: None,
                server: None,
                server_url: None,
            },
        ]);
        r1.raw_responses = Some(json!({
            "200": {
                "headers": {
                    "X-H1": {
                        "content": { "text/plain": { "schema": { "minLength": 2 } } }
                    },
                    "X-H2": {
                        "schema": { "minLength": 3 }
                    }
                }
            }
        }));
        r1.callbacks = vec![
            ParsedCallback {
                name: "onCb".to_string(),
                expression: RuntimeExpression::new("{$request.query.cb}"),
                method: "CUSTOM_CB".to_string(),
                path_params: r1.path_params.clone(),
                params: vec![r1.params[1].clone()],
                request_body: r1.request_body.clone(),
                security: vec![sec_group_oauth.clone()],
                security_defined: true,
                response_type: None,
                response_status: None,
                response_summary: None,
                response_description: None,
                response_media_type: None,
                response_example: None,
                response_headers: vec![],
            },
            ParsedCallback {
                name: "onCb".to_string(),
                expression: RuntimeExpression::new("{$request.query.cb}"),
                method: "POST".to_string(),
                path_params: r1.path_params.clone(),
                params: vec![],
                request_body: None,
                security: vec![],
                security_defined: false,
                response_type: None,
                response_status: None,
                response_summary: None,
                response_description: None,
                response_media_type: None,
                response_example: None,
                response_headers: vec![],
            },
        ];

        // Second route on same path to test existing path_extensions, raw_request_body without request_body, raw_responses non-object
        let mut r2 = r1.clone();
        r2.method = "GET".to_string();
        r2.servers_override = Some(vec![]);
        r2.path_servers = None;
        r2.base_path = Some("   ".to_string());
        r2.request_body = None;
        r2.raw_request_body = Some(json!({ "description": "raw only" }));
        r2.raw_responses = Some(json!("not_an_obj"));
        r2.response_status = None;
        r2.response_links = Some(vec![]);
        r2.callbacks = vec![];
        let mut r2_empty_raw_resp = r2.clone();
        r2_empty_raw_resp.method = "HEAD".to_string();
        r2_empty_raw_resp.servers_override = None;
        r2_empty_raw_resp.raw_responses = Some(json!({}));
        let _ = generate_openapi_document_with_routes(
            &[],
            std::slice::from_ref(&r2_empty_raw_resp),
            None,
            &rich_info,
        )
        .expect("with rich_info");

        // Third route as Webhook with non-object raw_request_body and request_body with non-array sequential schema
        let mut r3 = r2.clone();
        r3.kind = RouteKind::Webhook;
        r3.path = "myHook".to_string();
        r3.servers_override = None;
        r3.base_path = Some("/v3".to_string());
        r3.raw_request_body = Some(json!("not_an_obj"));
        r3.request_body = Some(RequestBodyDefinition {
            ty: "String".to_string(),
            description: Some("Desc".to_string()),
            media_type: "application/jsonl".to_string(),
            format: BodyFormat::Json,
            required: false,
            encoding: None,
            prefix_encoding: Some(vec![]),
            item_encoding: None,
            example: None,
        });
        r3.response_type = Some("String".to_string());
        r3.response_media_type = Some("application/jsonl".to_string());

        let mut r4 = r3.clone();
        r4.method = "POST".to_string();
        r4.base_path = Some("/v4".to_string());
        r4.raw_request_body = Some(json!({ "x-rb": 1 }));

        let raw_comps = json!({
            "schemas": {
                "Item": { "x-extra": true }
            },
            "parameters": {
                "P1": { "name": "p1", "in": "query" }
            }
        });
        let merged_doc = generate_openapi_document_with_routes_and_components(
            std::slice::from_ref(&simple_model),
            &[r1.clone(), r2.clone(), r3, r4],
            Some("https://spec.openapis.org/oas/3.1/dialect/base"),
            &OpenApiInfo::new("Merged", "1.0"),
            Some(&raw_comps),
        )
        .expect("merged_doc");
        assert!(merged_doc["components"]["schemas"]["Item"]["x-extra"] == true);
        let mut r_no_sec = r2_empty_raw_resp;
        r_no_sec.security = vec![];
        r_no_sec.security_defined = false;
        let _ = generate_openapi_document_with_routes_and_components(
            &[],
            &[r_no_sec],
            None,
            &OpenApiInfo::new("MergedNoModels", "1.0"),
            Some(&json!({})),
        )
        .expect("empty raw comps");
        let _ = generate_openapi_document_with_routes_and_components(
            &[],
            std::slice::from_ref(&r2),
            None,
            &OpenApiInfo::new("MergedNoModelsNonEmptySec", "1.0"),
            Some(&json!({})),
        )
        .expect("no models with sec");
        let _ = generate_openapi_document_with_routes_and_components(
            &[],
            std::slice::from_ref(&r2),
            None,
            &OpenApiInfo::new("MergedNoneComps", "1.0"),
            None,
        )
        .expect("none comps");

        // Conflicting path extensions & invalid path extension key
        let mut r_conflict = r1.clone();
        r_conflict.method = "DELETE".to_string();
        r_conflict
            .path_extensions
            .insert("x-pe".to_string(), json!("different"));
        assert!(generate_openapi_document_with_routes(
            &[],
            &[r1.clone(), r_conflict],
            None,
            &OpenApiInfo::new("T", "1")
        )
        .is_err());

        let mut r_bad_pext = r1.clone();
        r_bad_pext
            .path_extensions
            .insert("bad_key".to_string(), json!(1));
        assert!(generate_openapi_document_with_routes(
            &[],
            &[r_bad_pext],
            None,
            &OpenApiInfo::new("T", "1")
        )
        .is_err());

        // Conflicting security scheme in route vs callback
        let mut r_sec_conflict = r1.clone();
        r_sec_conflict.callbacks[0].security[0].schemes[3].scheme = Some(SecuritySchemeInfo {
            kind: SecuritySchemeKind::MutualTls,
            description: Some("conflict".to_string()),
            deprecated: false,
        });
        assert!(generate_openapi_document_with_routes(
            &[],
            &[r_sec_conflict],
            None,
            &OpenApiInfo::new("T", "1")
        )
        .is_err());

        let mut r_sec_conflict2 = r2.clone();
        r_sec_conflict2.security = vec![crate::openapi::parse::models::SecurityRequirementGroup {
            schemes: vec![SecurityRequirement {
                scheme_name: "MtlsScheme".to_string(),
                scopes: vec![],
                scheme: Some(SecuritySchemeInfo {
                    kind: SecuritySchemeKind::MutualTls,
                    description: Some("conflict2".to_string()),
                    deprecated: false,
                }),
            }],
        }];
        assert!(generate_openapi_document_with_routes(
            &[],
            &[r1, r_sec_conflict2],
            None,
            &OpenApiInfo::new("T", "1")
        )
        .is_err());

        // 3. Tuple struct with skipped field, Struct with deprecated/externalDocs/readOnly/writeOnly/rename_all,
        // Enum with rename/deprecated/externalDocs/rename_all/discriminator_default_mapping,
        // Type mapping edge cases (BTreeMap<String, Vec<Decimal>>, non-2-arg HashMap, unclosed generic `Vec<String`, `&[u8]`, `[u8]`, `Bytes`, `ByteBuf`, `bytes::Bytes`, `serde_bytes::ByteBuf`)
        let mut tuple_f0 = make_field("0", "String", None);
        tuple_f0.is_skipped = true;
        let tuple_f1 = make_field("1", "Decimal", None);
        let mut tuple_s = make_struct("MyTuple", vec![tuple_f0, tuple_f1]);
        tuple_s.rename = Some("RenamedTuple".to_string());
        tuple_s.is_deprecated = true;
        tuple_s.external_docs = Some(ParsedExternalDocs {
            url: "https://example.com/tuple".to_string(),
            description: Some("TupleDoc".to_string()),
        });
        let tuple_schema =
            generate_json_schema(&ParsedModel::Struct(tuple_s), None).expect("tuple");
        assert_eq!(tuple_schema["title"], json!("RenamedTuple"));
        assert_eq!(tuple_schema["type"], json!("array"));
        assert_eq!(tuple_schema["deprecated"], json!(true));

        let mut f_ro = make_field("ro_field", "BTreeMap<String, Vec<Decimal>>", Some("f desc"));
        f_ro.is_read_only = true;
        f_ro.is_write_only = true;
        f_ro.is_deprecated = true;
        f_ro.external_docs = Some(ParsedExternalDocs {
            url: "https://example.com/f".to_string(),
            description: None,
        });
        let mut f_skip = make_field("skip_me", "String", None);
        f_skip.is_skipped = true;
        let mut f_no_desc = make_field("b1", "&[u8]", None);
        f_no_desc.description = None;
        let mut named_s = make_struct(
            "Named",
            vec![
                f_ro,
                f_skip,
                f_no_desc,
                make_field("b2", "[u8]", None),
                make_field("b3", "Bytes", None),
                make_field("b4", "ByteBuf", None),
                make_field("b5", "bytes::Bytes", None),
                make_field("b6", "serde_bytes::ByteBuf", None),
                make_field("flt", "f32", None),
                make_field("nd", "NaiveDate", None),
                make_field("ndt", "NaiveDateTime", None),
                make_field("m_bad", "HashMap<String>", None),
                make_field("unclosed", "Vec<String", None),
                make_field("opt_ref", "Option<CustomType>", None),
                make_field("nested_map", "HashMap<String, Result<i32, String>>", None),
                make_field("empty_gen", "HashMap<,>", None),
            ],
        );
        named_s.description = None;
        named_s.rename_all = Some(RenameRule::CamelCase);
        named_s.deny_unknown_fields = true;
        let named_schema =
            generate_json_schema(&ParsedModel::Struct(named_s), None).expect("named");
        assert_eq!(named_schema["additionalProperties"], json!(false));
        assert!(named_schema["properties"].get("skipMe").is_none());
        let empty_schema = generate_json_schema(
            &ParsedModel::Struct(make_struct("EmptyStruct", vec![])),
            None,
        )
        .expect("empty_s");
        assert_eq!(empty_schema["type"], json!("object"));

        let rich_enum = ParsedEnum {
            name: "RichEnum".to_string(),
            description: Some("EDesc".to_string()),
            rename: Some("RenamedEnum".to_string()),
            rename_all: Some(RenameRule::SnakeCase),
            tag: Some("kind".to_string()),
            untagged: false,
            variants: vec![
                ParsedVariant {
                    name: "UnitVar".to_string(),
                    ty: None,
                    description: None,
                    rename: None,
                    aliases: None,
                    is_deprecated: true,
                },
                ParsedVariant {
                    name: "TypedVar".to_string(),
                    ty: Some("i64".to_string()),
                    description: None,
                    rename: Some("custom_typed".to_string()),
                    aliases: None,
                    is_deprecated: true,
                },
            ],
            is_deprecated: true,
            external_docs: Some(ParsedExternalDocs {
                url: "https://example.com/enum".to_string(),
                description: None,
            }),
            discriminator_mapping: Some(BTreeMap::new()),
            discriminator_default_mapping: Some("#/components/schemas/Fallback".to_string()),
        };
        let enum_schema =
            generate_json_schema(&ParsedModel::Enum(rich_enum), None).expect("rich_enum");
        assert_eq!(enum_schema["title"], json!("RenamedEnum"));
        assert_eq!(
            enum_schema["discriminator"]["defaultMapping"],
            json!("#/components/schemas/Fallback")
        );
        assert!(schema_item_schema(&json!({ "type": "string" })).is_none());
        assert!(is_sequential_media_type("text/event-stream"));
        assert!(is_sequential_media_type("multipart/byteranges"));
    }
}
