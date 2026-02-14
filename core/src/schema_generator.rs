#![deny(missing_docs)]

//! # Schema Generator
//!
//! Utilities for converting parsed Rust structs and enums into JSON Schema definitions.
//! This module enables the generation of OpenAPI-compliant schemas directly from
//! Rust source code models, respecting Serde attributes like `rename`, `rename_all`,
//! `deny_unknown_fields`, `skip`, `skip_serializing`, `skip_deserializing`, `tag`, and `untagged`.

use crate::error::AppError;
use crate::error::AppResult;
use crate::oas::models::{
    ExampleValue, LinkParamValue, LinkRequestBody, ParamSource, ParamStyle, ParsedLink,
    ParsedRoute, ParsedServer, ParsedServerVariable, RouteKind, RouteParam, SecuritySchemeInfo,
    SecuritySchemeKind,
};
use crate::parser::{
    ParsedEnum, ParsedExternalDocs, ParsedField, ParsedModel, ParsedStruct, RenameRule,
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
    pub security: Vec<crate::oas::models::SecurityRequirementGroup>,
    /// Specification extensions (`x-...`) attached to the Paths Object.
    pub paths_extensions: BTreeMap<String, Value>,
    /// Specification extensions (`x-...`) attached to the Webhooks Object.
    pub webhooks_extensions: BTreeMap<String, Value>,
    /// Specification extensions (`x-...`) to emit at the OpenAPI root.
    pub extensions: BTreeMap<String, Value>,
}

impl OpenApiInfo {
    /// Creates a new OpenApiInfo with required fields.
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
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Sets an optional description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the Terms of Service URL.
    pub fn with_terms_of_service(mut self, terms: impl Into<String>) -> Self {
        self.terms_of_service = Some(terms.into());
        self
    }

    /// Sets optional contact metadata.
    pub fn with_contact(mut self, contact: OpenApiContact) -> Self {
        self.contact = Some(contact);
        self
    }

    /// Sets optional license metadata.
    pub fn with_license(mut self, license: OpenApiLicense) -> Self {
        self.license = Some(license);
        self
    }

    /// Sets the `$self` URI for the OpenAPI document.
    pub fn with_self_uri(mut self, self_uri: impl Into<String>) -> Self {
        self.self_uri = Some(self_uri.into());
        self
    }

    /// Sets top-level external documentation for the generated OpenAPI document.
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
    pub fn with_server(mut self, server: OpenApiServer) -> Self {
        self.servers.push(server);
        self
    }

    /// Replaces the server list for the OpenAPI document.
    pub fn with_servers(mut self, servers: Vec<OpenApiServer>) -> Self {
        self.servers = servers;
        self
    }

    /// Adds a tag definition to the OpenAPI document.
    pub fn with_tag(mut self, tag: OpenApiTag) -> Self {
        self.tags.push(tag);
        self
    }

    /// Replaces the tag definitions for the OpenAPI document.
    pub fn with_tags(mut self, tags: Vec<OpenApiTag>) -> Self {
        self.tags = tags;
        self
    }

    /// Sets top-level security requirements.
    pub fn with_security(
        mut self,
        security: Vec<crate::oas::models::SecurityRequirementGroup>,
    ) -> Self {
        self.security = security;
        self
    }

    /// Adds a specification extension (`x-...`) to the Paths Object.
    pub fn with_paths_extension(mut self, key: impl Into<String>, value: Value) -> Self {
        self.paths_extensions.insert(key.into(), value);
        self
    }

    /// Replaces the Paths Object extensions to emit.
    pub fn with_paths_extensions(mut self, extensions: BTreeMap<String, Value>) -> Self {
        self.paths_extensions = extensions;
        self
    }

    /// Adds a specification extension (`x-...`) to the Webhooks Object.
    pub fn with_webhooks_extension(mut self, key: impl Into<String>, value: Value) -> Self {
        self.webhooks_extensions.insert(key.into(), value);
        self
    }

    /// Replaces the Webhooks Object extensions to emit.
    pub fn with_webhooks_extensions(mut self, extensions: BTreeMap<String, Value>) -> Self {
        self.webhooks_extensions = extensions;
        self
    }

    /// Adds a specification extension (`x-...`) to the OpenAPI root.
    pub fn with_extension(mut self, key: impl Into<String>, value: Value) -> Self {
        self.extensions.insert(key.into(), value);
        self
    }

    /// Replaces the specification extensions to emit at the OpenAPI root.
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
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the contact URL.
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Sets the contact email.
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
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            identifier: None,
            url: None,
        }
    }

    /// Sets the license SPDX identifier.
    pub fn with_identifier(mut self, identifier: impl Into<String>) -> Self {
        self.identifier = Some(identifier.into());
        self
    }

    /// Sets the license URL.
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
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Sets the tag description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the tag external documentation.
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
    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    /// Sets the tag kind.
    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }
}

/// Server metadata for OpenAPI `servers`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiServer {
    /// Server URL (may be relative).
    pub url: String,
    /// Optional description for the server.
    pub description: Option<String>,
    /// Optional unique name for the server.
    pub name: Option<String>,
    /// Optional variable definitions for server URL templating.
    pub variables: std::collections::BTreeMap<String, OpenApiServerVariable>,
}

impl OpenApiServer {
    /// Creates a new server with the required URL.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            description: None,
            name: None,
            variables: std::collections::BTreeMap::new(),
        }
    }

    /// Sets the server description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the server name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Adds a server variable definition.
    pub fn with_variable(
        mut self,
        name: impl Into<String>,
        variable: OpenApiServerVariable,
    ) -> Self {
        self.variables.insert(name.into(), variable);
        self
    }

    /// Replaces the server variables map.
    pub fn with_variables(
        mut self,
        variables: std::collections::BTreeMap<String, OpenApiServerVariable>,
    ) -> Self {
        self.variables = variables;
        self
    }
}

/// Server variable metadata for templated server URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiServerVariable {
    /// Allowed enum values (if constrained).
    pub enum_values: Option<Vec<String>>,
    /// Default value for substitution.
    pub default: String,
    /// Optional description.
    pub description: Option<String>,
}

impl OpenApiServerVariable {
    /// Creates a new server variable with the required default.
    pub fn new(default: impl Into<String>) -> Self {
        Self {
            enum_values: None,
            default: default.into(),
            description: None,
        }
    }

    /// Sets enum values for the variable.
    pub fn with_enum_values(mut self, values: Vec<String>) -> Self {
        self.enum_values = Some(values);
        self
    }

    /// Sets the variable description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

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

/// Generates a minimal OpenAPI 3.2 document containing the provided model as a component schema.
///
/// This is intended for Rust -> OpenAPI workflows where a single type is being reflected
/// into an OpenAPI description.
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

    let mut doc = Map::new();
    doc.insert("openapi".to_string(), json!("3.2.0"));
    insert_extensions(&mut doc, &info.extensions);
    if let Some(self_uri) = &info.self_uri {
        doc.insert("$self".to_string(), json!(self_uri));
    }
    if let Some(d) = dialect {
        doc.insert("jsonSchemaDialect".to_string(), json!(d));
    }
    doc.insert("info".to_string(), Value::Object(info_obj));
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
    doc.insert("info".to_string(), Value::Object(info_obj));
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

    if !doc.contains_key("components")
        && !doc.contains_key("paths")
        && !doc.contains_key("webhooks")
    {
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
        if let Some(doc_obj) = doc.as_object_mut() {
            doc_obj.insert("components".to_string(), Value::Object(merged));
        }
    }

    Ok(doc)
}

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

fn insert_route_operation(
    target: &mut Map<String, Value>,
    route: &ParsedRoute,
    emit_operation_servers: bool,
    root_security_empty: bool,
) -> AppResult<()> {
    let entry = target
        .entry(route.path.clone())
        .or_insert_with(|| Value::Object(Map::new()));

    let Value::Object(path_item) = entry else {
        return Ok(());
    };

    merge_path_extensions(path_item, &route.path_extensions, &route.path)?;
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
    if let Some(servers) = route.path_servers.as_ref() {
        if !servers.is_empty() {
            path_item
                .entry("servers".to_string())
                .or_insert_with(|| parsed_servers_value(servers));
        }
    }
    if !route.path_params.is_empty() && !path_item.contains_key("parameters") {
        let params = route
            .path_params
            .iter()
            .map(build_parameter)
            .collect::<Vec<_>>();
        if !params.is_empty() {
            path_item.insert("parameters".to_string(), Value::Array(params));
        }
    }

    let op = build_operation(route, emit_operation_servers, root_security_empty);
    let method_key = route.method.to_ascii_lowercase();
    if is_reserved_method(&method_key) {
        path_item.insert(method_key, op);
        return Ok(());
    }

    let additional = path_item
        .entry("additionalOperations".to_string())
        .or_insert_with(|| Value::Object(Map::new()));

    if let Value::Object(map) = additional {
        map.insert(route.method.clone(), op);
    }
    Ok(())
}

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
            op.insert("servers".to_string(), parsed_servers_value(servers));
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

fn insert_extensions(target: &mut Map<String, Value>, extensions: &BTreeMap<String, Value>) {
    for (key, value) in extensions {
        if key.starts_with("x-") {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn servers_value(servers: &[OpenApiServer]) -> Value {
    let entries = servers.iter().map(server_value).collect::<Vec<_>>();
    Value::Array(entries)
}

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

fn parsed_servers_value(servers: &[ParsedServer]) -> Value {
    let entries = servers.iter().map(parsed_server_value).collect::<Vec<_>>();
    Value::Array(entries)
}

fn parsed_server_value(server: &ParsedServer) -> Value {
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
            vars.insert(var_name.clone(), parsed_server_variable_value(var));
        }
        obj.insert("variables".to_string(), Value::Object(vars));
    }
    Value::Object(obj)
}

fn parsed_server_variable_value(variable: &ParsedServerVariable) -> Value {
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

fn collect_security_schemes(routes: &[ParsedRoute]) -> AppResult<Map<String, Value>> {
    let mut seen: std::collections::HashMap<String, SecuritySchemeInfo> =
        std::collections::HashMap::new();

    for route in routes {
        for group in &route.security {
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
        for callback in &route.callbacks {
            for group in &callback.security {
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

fn oauth_flows_value(flows: &crate::oas::models::OAuthFlows) -> Value {
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

fn oauth_flow_value(flow: &crate::oas::models::OAuthFlow) -> Value {
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

fn api_key_location(source: &ParamSource) -> Option<&'static str> {
    match source {
        ParamSource::Header => Some("header"),
        ParamSource::Query => Some("query"),
        ParamSource::Cookie => Some("cookie"),
        _ => None,
    }
}

/// Collects unique, non-empty base paths from parsed routes.
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
        crate::oas::models::ExampleKind::Data => {
            if has_meta {
                example_obj.insert("dataValue".to_string(), example.value.clone());
                target.insert("examples".to_string(), json!({ "example": example_obj }));
            } else {
                target.insert("example".to_string(), example.value.clone());
            }
        }
        crate::oas::models::ExampleKind::Serialized => {
            let serialized = match &example.value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            example_obj.insert("serializedValue".to_string(), json!(serialized));
            target.insert("examples".to_string(), json!({ "example": example_obj }));
        }
        crate::oas::models::ExampleKind::External => {
            let external = match &example.value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            example_obj.insert("externalValue".to_string(), json!(external));
            target.insert("examples".to_string(), json!({ "example": example_obj }));
        }
    }
}

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

    let content_media = param
        .content_media_type
        .as_ref()
        .map(|m| m.as_str())
        .or_else(|| match param.source {
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
                if let Value::Object(base_obj) = base_val {
                    if let Value::Object(raw_obj) = raw_val {
                        let merged = merge_json_objects(base_obj.clone(), raw_obj);
                        *base_val = Value::Object(merged);
                    }
                }
            }
        }
    }
    base
}

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

fn merge_media_with_raw(generated: Map<String, Value>, raw: Option<&Value>) -> Map<String, Value> {
    let mut out = raw.and_then(|v| v.as_object()).cloned().unwrap_or_default();
    for (key, gen_val) in generated {
        if key == "schema" || key == "itemSchema" {
            let merged = merge_schema_value(gen_val, out.get(&key));
            out.insert(key, merged);
        } else {
            out.insert(key, gen_val);
        }
    }
    out
}

fn merge_content_with_raw(generated: &Map<String, Value>, raw: Option<&Value>) -> Value {
    let mut out = raw.and_then(|v| v.as_object()).cloned().unwrap_or_default();
    for (media, gen_val) in generated {
        let gen_map = gen_val.as_object().cloned().unwrap_or_default();
        let merged_media = merge_media_with_raw(gen_map, out.get(media));
        out.insert(media.clone(), Value::Object(merged_media));
    }
    Value::Object(out)
}

fn merge_headers_with_raw(generated: &Map<String, Value>, raw: Option<&Value>) -> Value {
    let mut out = raw.and_then(|v| v.as_object()).cloned().unwrap_or_default();
    for (name, gen_val) in generated {
        let gen_obj = gen_val.as_object().cloned().unwrap_or_default();
        let mut merged = out
            .get(name)
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        for (key, header_val) in gen_obj {
            if key == "schema" {
                let merged_schema = merge_schema_value(header_val, merged.get("schema"));
                merged.insert("schema".to_string(), merged_schema);
            } else if key == "content" {
                let gen_content = header_val.as_object().cloned().unwrap_or_default();
                let merged_content = merge_content_with_raw(&gen_content, merged.get("content"));
                merged.insert("content".to_string(), merged_content);
            } else {
                merged.insert(key, header_val);
            }
        }
        out.insert(name.clone(), Value::Object(merged));
    }
    Value::Object(out)
}

fn merge_response_entry_with_raw(generated: Value, raw: Option<&Value>) -> Value {
    let Some(raw_obj) = raw.and_then(|v| v.as_object()) else {
        return generated;
    };
    let gen_obj = match generated {
        Value::Object(map) => map,
        other => return other,
    };
    let mut out = raw_obj.clone();
    for (key, gen_val) in gen_obj {
        if key == "content" {
            let gen_map = gen_val.as_object().cloned().unwrap_or_default();
            let merged = merge_content_with_raw(&gen_map, out.get("content"));
            out.insert("content".to_string(), merged);
        } else if key == "headers" {
            let gen_map = gen_val.as_object().cloned().unwrap_or_default();
            let merged = merge_headers_with_raw(&gen_map, out.get("headers"));
            out.insert("headers".to_string(), merged);
        } else {
            out.insert(key, gen_val);
        }
    }
    Value::Object(out)
}

fn merge_request_body(
    raw: &Value,
    body: Option<&crate::oas::models::RequestBodyDefinition>,
) -> Value {
    let Some(body) = body else {
        return raw.clone();
    };

    let Some(raw_obj) = raw.as_object() else {
        return build_request_body(body);
    };

    let mut merged = raw_obj.clone();
    if let Some(desc) = &body.description {
        merged.insert("description".to_string(), json!(desc));
    }
    if body.required {
        merged.insert("required".to_string(), json!(true));
    }

    let mut content = merged
        .get("content")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let generated_media = build_request_body_media(body);
    let merged_media = merge_media_with_raw(generated_media, content.get(&body.media_type));
    content.insert(body.media_type.clone(), Value::Object(merged_media));
    merged.insert("content".to_string(), Value::Object(content));
    Value::Object(merged)
}

fn build_request_body(body: &crate::oas::models::RequestBodyDefinition) -> Value {
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

fn build_request_body_media(
    body: &crate::oas::models::RequestBodyDefinition,
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

fn build_responses(route: &ParsedRoute) -> Value {
    let status = route.response_status.as_deref().unwrap_or("200");
    let entry = build_response_entry(route);

    if let Some(raw) = route.raw_responses.as_ref() {
        if let Some(raw_obj) = raw.as_object() {
            let mut merged_value = Value::Object(raw_obj.clone());
            rehydrate_header_content(&mut merged_value);
            let mut merged = match merged_value {
                Value::Object(map) => map,
                _ => raw_obj.clone(),
            };
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

fn build_response_entry(route: &ParsedRoute) -> Value {
    let mut resp = Map::new();
    if let Some(summary) = route.response_summary.as_ref() {
        resp.insert("summary".to_string(), json!(summary));
    }
    let description = route.response_description.as_deref().unwrap_or("OK");
    let media_type = route
        .response_media_type
        .as_deref()
        .unwrap_or("application/json");

    resp.insert("description".to_string(), json!(description));

    if route.response_type.is_some() || route.response_example.is_some() {
        let mut media = Map::new();
        if let Some(ty) = &route.response_type {
            let (schema, _) = schema_from_rust_type(ty);
            if is_sequential_media_type(media_type) {
                if let Some(item_schema) = schema_item_schema(&schema) {
                    media.insert("itemSchema".to_string(), item_schema);
                }
            }
            media.insert("schema".to_string(), schema);
        }
        if let Some(example) = &route.response_example {
            insert_example_value(&mut media, example);
        }
        let mut content = Map::new();
        content.insert(media_type.to_string(), Value::Object(media));
        resp.insert("content".to_string(), Value::Object(content));
    }

    if !route.response_headers.is_empty() {
        let mut headers = Map::new();
        for header in &route.response_headers {
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

    if let Some(links) = route.response_links.as_ref() {
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

fn rehydrate_header_content(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(headers_val) = map.get_mut("headers") {
                if let Some(headers_map) = headers_val.as_object_mut() {
                    for (_, header_val) in headers_map.iter_mut() {
                        if let Some(obj) = header_val.as_object_mut() {
                            if obj.contains_key("content") {
                                obj.remove("x-cdd-content");
                            } else if let Some(content) = obj.remove("x-cdd-content") {
                                obj.insert("content".to_string(), content);
                            }
                        }
                    }
                }
            }

            for v in map.values_mut() {
                rehydrate_header_content(v);
            }
        }
        Value::Array(items) => {
            for v in items.iter_mut() {
                rehydrate_header_content(v);
            }
        }
        _ => {}
    }
}

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
        obj.insert("server".to_string(), parsed_server_value(server));
    } else if let Some(server_url) = &link.server_url {
        obj.insert("server".to_string(), json!({ "url": server_url }));
    }
    Value::Object(obj)
}

fn build_callbacks(
    callbacks: &[crate::oas::models::ParsedCallback],
    root_security_empty: bool,
) -> Value {
    let mut cb_map = Map::new();

    for callback in callbacks {
        let entry = cb_map
            .entry(callback.name.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        let Value::Object(expr_map) = entry else {
            continue;
        };

        let expr_key = callback.expression.as_str().to_string();
        let expr_entry = expr_map
            .entry(expr_key)
            .or_insert_with(|| Value::Object(Map::new()));
        let Value::Object(path_item) = expr_entry else {
            continue;
        };

        if !callback.path_params.is_empty() && !path_item.contains_key("parameters") {
            let params = callback
                .path_params
                .iter()
                .map(build_parameter)
                .collect::<Vec<_>>();
            if !params.is_empty() {
                path_item.insert("parameters".to_string(), Value::Array(params));
            }
        }

        insert_callback_operation(path_item, callback, root_security_empty);
    }

    Value::Object(cb_map)
}

fn insert_callback_operation(
    path_item: &mut Map<String, Value>,
    callback: &crate::oas::models::ParsedCallback,
    root_security_empty: bool,
) {
    let emit_security =
        callback.security_defined || (root_security_empty && !callback.security.is_empty());
    let op = build_callback_operation(callback, emit_security);
    let method_key = callback.method.to_ascii_lowercase();
    if is_reserved_method(&method_key) {
        path_item.insert(method_key, op);
        return;
    }

    let additional = path_item
        .entry("additionalOperations".to_string())
        .or_insert_with(|| Value::Object(Map::new()));

    if let Value::Object(map) = additional {
        map.insert(callback.method.clone(), op);
    }
}

fn build_callback_operation(
    callback: &crate::oas::models::ParsedCallback,
    emit_security: bool,
) -> Value {
    let mut op = Map::new();

    if !callback.params.is_empty() {
        let params = callback
            .params
            .iter()
            .map(build_parameter)
            .collect::<Vec<_>>();
        if !params.is_empty() {
            op.insert("parameters".to_string(), Value::Array(params));
        }
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

fn build_callback_responses(callback: &crate::oas::models::ParsedCallback) -> Value {
    let mut responses = Map::new();
    let mut resp = Map::new();
    let status = callback.response_status.as_deref().unwrap_or("200");
    if let Some(summary) = callback.response_summary.as_ref() {
        resp.insert("summary".to_string(), json!(summary));
    }
    let description = callback.response_description.as_deref().unwrap_or("OK");
    let media_type = callback
        .response_media_type
        .as_deref()
        .unwrap_or("application/json");

    resp.insert("description".to_string(), json!(description));

    if callback.response_type.is_some() || callback.response_example.is_some() {
        let mut media = Map::new();
        if let Some(ty) = &callback.response_type {
            let (schema, _) = schema_from_rust_type(ty);
            if is_sequential_media_type(media_type) {
                if let Some(item_schema) = schema_item_schema(&schema) {
                    media.insert("itemSchema".to_string(), item_schema);
                }
            }
            media.insert("schema".to_string(), schema);
        }
        if let Some(example) = &callback.response_example {
            insert_example_value(&mut media, example);
        }
        let mut content = Map::new();
        content.insert(media_type.to_string(), Value::Object(media));
        resp.insert("content".to_string(), Value::Object(content));
    }

    if !callback.response_headers.is_empty() {
        let mut headers = Map::new();
        for header in &callback.response_headers {
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

    responses.insert(status.to_string(), Value::Object(resp));
    Value::Object(responses)
}

fn build_security(requirements: &[crate::oas::models::SecurityRequirementGroup]) -> Value {
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

fn encoding_info_value(info: &crate::oas::models::EncodingInfo) -> Value {
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

fn schema_from_rust_type(ty: &str) -> (Value, bool) {
    if is_binary_type(ty) {
        return (binary_schema_value(), is_optional_type(ty));
    }
    let parsed = parse_rust_type(ty);
    (shape_to_schema(&parsed.shape), parsed.is_optional)
}

fn param_source_str(source: &ParamSource) -> &'static str {
    match source {
        ParamSource::Path => "path",
        ParamSource::Query => "query",
        ParamSource::QueryString => "querystring",
        ParamSource::Header => "header",
        ParamSource::Cookie => "cookie",
    }
}

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

fn is_reserved_method(method: &str) -> bool {
    matches!(
        method,
        "get" | "post" | "put" | "delete" | "patch" | "options" | "head" | "trace" | "query"
    )
}

fn normalize_media_type(media_type: &str) -> String {
    media_type
        .split(';')
        .next()
        .unwrap_or(media_type)
        .trim()
        .to_ascii_lowercase()
}

fn is_sequential_media_type(media_type: &str) -> bool {
    let normalized = normalize_media_type(media_type);
    matches!(
        normalized.as_str(),
        "application/jsonl"
            | "application/x-ndjson"
            | "application/json-seq"
            | "application/geo+json-seq"
            | "text/event-stream"
            | "multipart/mixed"
            | "multipart/byteranges"
    ) || normalized.ends_with("+jsonl")
        || normalized.ends_with("+ndjson")
        || normalized.ends_with("+json-seq")
}

fn schema_item_schema(schema: &Value) -> Option<Value> {
    let obj = schema.as_object()?;
    if obj.get("type").and_then(|v| v.as_str()) == Some("array") {
        return obj.get("items").cloned();
    }

    if let Some(any_of) = obj.get("anyOf").and_then(|v| v.as_array()) {
        for entry in any_of {
            if let Some(items) = schema_item_schema(entry) {
                return Some(items);
            }
        }
    }

    None
}

/// Generates schema for a struct.
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

            if field.is_deprecated {
                if let Some(obj) = field_schema.as_object_mut() {
                    obj.insert("deprecated".to_string(), json!(true));
                }
            }
            if let Some(ext) = &field.external_docs {
                if let Some(obj) = field_schema.as_object_mut() {
                    obj.insert("externalDocs".to_string(), external_docs_value(ext));
                }
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
            map_type_to_schema(ty)
        } else {
            // Unit variant -> Enum::A -> "A"
            json!({ "type": "string", "const": variant_name })
        };

        if variant.is_deprecated {
            if let Some(obj) = sub_schema.as_object_mut() {
                obj.insert("deprecated".to_string(), json!(true));
            }
        }

        one_of.push(sub_schema);
    }

    // Handle Untagged vs Tagged
    schema.insert("oneOf".to_string(), Value::Array(one_of));

    if let Some(tag) = &enum_def.tag {
        // Tagged enum: add discriminator hint (with optional mapping)
        let mut discriminator = Map::new();
        discriminator.insert("propertyName".to_string(), json!(tag));
        if let Some(mapping) = &enum_def.discriminator_mapping {
            if !mapping.is_empty() {
                discriminator.insert("mapping".to_string(), json!(mapping));
            }
        }
        if let Some(default_mapping) = &enum_def.discriminator_default_mapping {
            if !default_mapping.is_empty() {
                discriminator.insert("defaultMapping".to_string(), json!(default_mapping));
            }
        }
        schema.insert("discriminator".to_string(), Value::Object(discriminator));
    }

    Ok(Value::Object(schema))
}

fn external_docs_value(ext: &ParsedExternalDocs) -> Value {
    let mut map = Map::new();
    map.insert("url".to_string(), json!(ext.url));
    if let Some(desc) = &ext.description {
        map.insert("description".to_string(), json!(desc));
    }
    Value::Object(map)
}

/// Processes a single field to determine its JSON name, schema, and optionality.
fn process_field(field: &ParsedField, rename_all: Option<&RenameRule>) -> (String, Value, bool) {
    // 1. Determine Name
    let name = field
        .rename
        .clone()
        .or_else(|| rename_all.map(|rule| rule.apply(&field.name)))
        .unwrap_or_else(|| field.name.clone());

    // 2. Parse Type (special-case binary payloads)
    let (mut schema, is_optional) = if is_binary_type(&field.ty) {
        (binary_schema_value(), is_optional_type(&field.ty))
    } else {
        let parsed = parse_rust_type(&field.ty);
        let schema = shape_to_schema(&parsed.shape);
        (schema, parsed.is_optional)
    };

    if is_optional {
        schema = make_nullable_schema(schema);
    }

    // 5. Add Description
    if let Some(desc) = &field.description {
        if let Some(obj) = schema.as_object_mut() {
            obj.insert("description".to_string(), json!(desc));
        }
    }

    if let Some(obj) = schema.as_object_mut() {
        if field.is_read_only {
            obj.insert("readOnly".to_string(), json!(true));
        }
        if field.is_write_only {
            obj.insert("writeOnly".to_string(), json!(true));
        }
    }

    (name, schema, is_optional)
}

fn binary_schema_value() -> Value {
    json!({
        "type": "string",
        "contentEncoding": "base64",
        "contentMediaType": "application/octet-stream"
    })
}

fn make_nullable_schema(schema: Value) -> Value {
    let Some(obj) = schema.as_object() else {
        return json!({ "anyOf": [schema, { "type": "null" }] });
    };

    if obj.contains_key("$ref") {
        return json!({ "anyOf": [schema, { "type": "null" }] });
    }

    if let Some(type_val) = obj.get("type") {
        let mut types = match type_val {
            Value::String(s) => vec![Value::String(s.clone())],
            Value::Array(arr) => arr.clone(),
            _ => Vec::new(),
        };

        if !types
            .iter()
            .any(|t| matches!(t, Value::String(s) if s == "null"))
        {
            types.push(Value::String("null".to_string()));
        }

        let mut new_obj = obj.clone();
        new_obj.insert("type".to_string(), Value::Array(types));
        return Value::Object(new_obj);
    }

    json!({ "anyOf": [schema, { "type": "null" }] })
}

fn is_optional_type(ty: &str) -> bool {
    let ty = ty.trim();
    ty.starts_with("Option<") && ty.ends_with('>')
}

fn is_binary_type(ty: &str) -> bool {
    let ty = ty.trim();
    if is_optional_type(ty) {
        let inner = &ty[7..ty.len() - 1];
        return is_binary_type(inner);
    }

    if ty.starts_with("Vec<") && ty.ends_with('>') {
        let inner = ty[4..ty.len() - 1].trim();
        return inner == "u8";
    }

    if ty == "&[u8]" || ty == "[u8]" {
        return true;
    }

    ty == "Bytes" || ty == "ByteBuf" || ty.ends_with("::Bytes") || ty.ends_with("::ByteBuf")
}

/// Parses the Rust type string to identify wrappers like `Option<...>` and `Vec<...>`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeShape {
    Base(String),
    Vec(Box<TypeShape>),
    Map(Box<TypeShape>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedType {
    shape: TypeShape,
    is_optional: bool,
}

/// Parses the Rust type string to identify wrappers like `Option<...>`, `Vec<...>`,
/// and map-like containers (`HashMap`/`BTreeMap`).
fn parse_rust_type(ty: &str) -> ParsedType {
    parse_rust_type_inner(ty.trim(), true)
}

fn parse_rust_type_inner(ty: &str, allow_optional: bool) -> ParsedType {
    if allow_optional {
        if let Some(inner) = strip_generic(ty, "Option") {
            let parsed = parse_rust_type_inner(inner, false);
            return ParsedType {
                shape: parsed.shape,
                is_optional: true,
            };
        }
    }

    if let Some(inner) = strip_generic(ty, "Vec") {
        let parsed = parse_rust_type_inner(inner, false);
        return ParsedType {
            shape: TypeShape::Vec(Box::new(parsed.shape)),
            is_optional: false,
        };
    }

    if let Some(map_inner) = strip_map_generic(ty) {
        let parsed = parse_rust_type_inner(map_inner.as_str(), false);
        return ParsedType {
            shape: TypeShape::Map(Box::new(parsed.shape)),
            is_optional: false,
        };
    }

    ParsedType {
        shape: TypeShape::Base(ty.to_string()),
        is_optional: false,
    }
}

fn strip_generic<'a>(ty: &'a str, target: &str) -> Option<&'a str> {
    let (base, inner) = split_generic(ty)?;
    if base == target {
        Some(inner)
    } else {
        None
    }
}

fn strip_map_generic(ty: &str) -> Option<String> {
    let (base, inner) = split_generic(ty)?;
    if base == "HashMap" || base == "BTreeMap" {
        let args = split_generic_args(inner);
        if args.len() == 2 {
            return Some(args[1].trim().to_string());
        }
    }
    None
}

fn split_generic<'a>(ty: &'a str) -> Option<(&'a str, &'a str)> {
    let start = ty.find('<')?;
    if !ty.ends_with('>') {
        return None;
    }
    let base = ty[..start].trim();
    let base = base.split("::").last().unwrap_or(base);
    let inner = &ty[start + 1..ty.len() - 1];
    Some((base, inner))
}

fn split_generic_args(inner: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();

    for ch in inner.chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                if depth > 0 {
                    depth -= 1;
                }
                current.push(ch);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }

    args
}

fn shape_to_schema(shape: &TypeShape) -> Value {
    match shape {
        TypeShape::Base(ty) => map_type_to_schema(ty),
        TypeShape::Vec(inner) => json!({
            "type": "array",
            "items": shape_to_schema(inner)
        }),
        TypeShape::Map(inner) => json!({
            "type": "object",
            "additionalProperties": shape_to_schema(inner)
        }),
    }
}

/// Maps a "clean" Rust type to a base JSON Schema object.
fn map_type_to_schema(ty: &str) -> Value {
    match ty {
        "i8" | "i16" | "i32" | "u8" | "u16" | "u32" | "isize" | "usize" => {
            json!({ "type": "integer", "format": "int32" })
        }
        "i64" | "u64" => json!({ "type": "integer", "format": "int64" }),

        "f32" | "f64" => json!({ "type": "number" }),

        "bool" => json!({ "type": "boolean" }),

        "String" | "&str" | "char" => json!({ "type": "string" }),
        "Uuid" => json!({ "type": "string", "format": "uuid" }),
        "NaiveDate" => json!({ "type": "string", "format": "date" }),
        "NaiveDateTime" | "DateTime<Utc>" | "DateTime<Local>" => {
            json!({ "type": "string", "format": "date-time" })
        }
        "Decimal" => json!({ "type": "string", "format": "decimal" }),

        // Fallback
        other => json!({ "$ref": format!("#/components/schemas/{}", other) }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::{
        BodyFormat, EncodingInfo, ExampleValue, ParsedCallback, RequestBodyDefinition,
        ResponseHeader, RuntimeExpression,
    };
    use crate::oas::models::{
        ParamSource, ParamStyle, ParsedRoute, ParsedServer, ParsedServerVariable, RouteKind,
        RouteParam, SecurityRequirement,
    };
    use crate::parser::{ParsedField, ParsedVariant};
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
        let schema = generate_json_schema(&def, None).unwrap();

        let props = schema["properties"].as_object().unwrap();
        assert_eq!(props["id"]["type"], "integer");
        assert_eq!(props["active"]["type"], "boolean");
        assert!(!schema.as_object().unwrap().contains_key("$schema"));
    }

    #[test]
    fn test_generate_tuple_schema() {
        let fields = vec![
            make_field("0", "i32", None),
            make_field("1", "String", None),
        ];

        let def = ParsedModel::Struct(make_struct("Point", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        assert_eq!(schema["type"], "array");
        assert_eq!(schema["items"], false);
        let prefix_items = schema["prefixItems"].as_array().unwrap();
        assert_eq!(prefix_items.len(), 2);
        assert_eq!(prefix_items[0]["type"], "integer");
        assert_eq!(prefix_items[1]["type"], "string");
    }

    #[test]
    fn test_generate_schema_with_dialect() {
        let fields = vec![make_field("id", "i32", None)];
        let def = ParsedModel::Struct(make_struct("User", fields));
        let dialect = "https://spec.openapis.org/oas/3.1/dialect/base";

        let schema = generate_json_schema(&def, Some(dialect)).unwrap();

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

        let schema = generate_json_schema(&ParsedModel::Struct(s), None).unwrap();
        let props = schema["properties"].as_object().unwrap();
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

        let doc = generate_openapi_document(&def, Some(dialect), &info).unwrap();
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

        let doc = generate_openapi_document(&def, None, &info).unwrap();
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
        let doc = generate_openapi_document(&def, None, &info).unwrap();
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

        let err = generate_openapi_document(&def, None, &info).unwrap_err();
        assert!(format!("{err}").contains("identifier"));
    }

    #[test]
    fn test_generate_openapi_document_rejects_invalid_paths_extension_key() {
        let fields = vec![make_field("id", "i32", None)];
        let def = ParsedModel::Struct(make_struct("User", fields));
        let info =
            OpenApiInfo::new("Test API", "1.0.0").with_paths_extension("paths-meta", json!(true));

        let err = generate_openapi_document(&def, None, &info).unwrap_err();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
        let tags = doc["tags"].as_array().unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
        assert_eq!(doc["servers"][0]["url"], "https://api.example.com/v1");
        assert_eq!(doc["servers"][0]["name"], "prod");
        assert_eq!(doc["servers"][0]["description"], "Production");
        assert!(doc["paths"]["/users"]["get"].get("servers").is_none());
    }

    #[test]
    fn test_generate_map_schema() {
        let fields = vec![make_field("tags", "HashMap<String, i32>", None)];
        let def = ParsedModel::Struct(make_struct("Tagged", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        let props = schema["properties"].as_object().unwrap();
        let tags = &props["tags"];
        assert_eq!(tags["type"], "object");
        assert_eq!(tags["additionalProperties"]["type"], "integer");
    }

    #[test]
    fn test_generate_nested_map_schema() {
        let fields = vec![make_field("meta", "BTreeMap<String, Vec<String>>", None)];
        let def = ParsedModel::Struct(make_struct("Meta", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        let props = schema["properties"].as_object().unwrap();
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

        let schema = generate_json_schema(&ParsedModel::Struct(s), None).unwrap();
        assert_eq!(schema["externalDocs"]["url"], "https://example.com/schema");
        assert_eq!(schema["externalDocs"]["description"], "Schema docs");

        let props = schema["properties"].as_object().unwrap();
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
        let schema = generate_json_schema(&def, None).unwrap();
        let props = schema["properties"].as_object().unwrap();
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
        let schema = generate_json_schema(&def, None).unwrap();

        assert!(schema.get("required").is_none());
        let labels = &schema["properties"]["labels"];
        let types = labels["type"].as_array().unwrap();
        assert!(types
            .iter()
            .any(|t| matches!(t, Value::String(s) if s == "null")));
    }

    #[test]
    fn test_optional_string_nullable_schema() {
        let fields = vec![make_field("nickname", "Option<String>", None)];
        let def = ParsedModel::Struct(make_struct("NullableUser", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        let nickname = &schema["properties"]["nickname"];
        let types = nickname["type"].as_array().unwrap();
        assert!(types
            .iter()
            .any(|t| matches!(t, Value::String(s) if s == "null")));
        assert!(types
            .iter()
            .any(|t| matches!(t, Value::String(s) if s == "string")));
    }

    #[test]
    fn test_optional_ref_schema_uses_anyof() {
        let fields = vec![make_field("owner", "Option<User>", None)];
        let def = ParsedModel::Struct(make_struct("Owned", fields));
        let schema = generate_json_schema(&def, None).unwrap();

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

        let schema = generate_json_schema(&ParsedModel::Enum(en), None).unwrap();
        assert!(schema["oneOf"].is_array());
        assert!(schema["discriminator"].is_object());
        assert_eq!(schema["discriminator"]["propertyName"], "type");
        assert!(!schema.as_object().unwrap().contains_key("$schema"));
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

        let schema = generate_json_schema(&ParsedModel::Enum(en), None).unwrap();
        let one_of = schema["oneOf"].as_array().unwrap();
        assert_eq!(one_of[0]["const"], "red-blue");
        assert_eq!(one_of[1]["const"], "green");
    }

    #[test]
    fn test_generate_struct_title_uses_rename() {
        let fields = vec![make_field("id", "i32", None)];
        let mut s = make_struct("User", fields);
        s.rename = Some("UserModel".to_string());

        let schema = generate_json_schema(&ParsedModel::Struct(s), None).unwrap();
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

        let schema = generate_json_schema(&ParsedModel::Enum(en), None).unwrap();
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
        let schema = generate_json_schema(&ParsedModel::Enum(en), Some(dialect)).unwrap();
        assert_eq!(schema["$schema"], dialect);
        assert_eq!(schema["title"], "Status");
    }

    #[test]
    fn test_generate_binary_vec_u8_schema() {
        let fields = vec![make_field("payload", "Vec<u8>", None)];
        let def = ParsedModel::Struct(make_struct("Upload", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        let payload = &schema["properties"]["payload"];
        assert_eq!(payload["type"], "string");
        assert_eq!(payload["contentEncoding"], "base64");
        assert_eq!(payload["contentMediaType"], "application/octet-stream");
    }

    #[test]
    fn test_generate_binary_bytes_schema() {
        let fields = vec![make_field("data", "bytes::Bytes", None)];
        let def = ParsedModel::Struct(make_struct("Blob", fields));
        let schema = generate_json_schema(&def, None).unwrap();

        let data = &schema["properties"]["data"];
        assert_eq!(data["type"], "string");
        assert_eq!(data["contentEncoding"], "base64");
    }

    #[test]
    fn test_generate_binary_optional_not_required() {
        let fields = vec![make_field("payload", "Option<Vec<u8>>", None)];
        let def = ParsedModel::Struct(make_struct("Upload", fields));
        let schema = generate_json_schema(&def, None).unwrap();

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

        let doc = generate_openapi_document_with_routes(&[model], &[route], None, &info).unwrap();

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
        .unwrap();

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
        .unwrap();

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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
        let path_item = &doc["paths"]["/items/{id}"];
        assert_eq!(path_item["summary"], "Path summary");
        assert_eq!(path_item["description"], "Path description");
        assert_eq!(path_item["x-path-meta"]["owner"], "api");
        assert_eq!(path_item["servers"][0]["url"], "https://api.example.com/v2");
        let path_params = path_item["parameters"].as_array().unwrap();
        assert!(path_params
            .iter()
            .any(|p| p["name"] == "id" && p["in"] == "path"));

        let op = &path_item["get"];
        assert_eq!(op["summary"], "Op summary");
        assert_eq!(op["description"], "Op description");
        assert!(op.get("servers").is_none());
        let op_params = op["parameters"].as_array().unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
        let header = &doc["paths"]["/status"]["get"]["responses"]["200"]["headers"]["X-Rate-Limit"];
        assert!(header.get("content").is_some());
        assert_eq!(header["content"]["text/plain"]["schema"]["type"], "integer");
        assert!(
            header["content"]["text/plain"].get("examples").is_some()
                || header["content"]["text/plain"].get("example").is_some()
        );
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

            request_body: Some(crate::oas::models::RequestBodyDefinition {
                ty: "Widget".into(),
                description: None,
                media_type: "application/json".into(),
                format: crate::oas::BodyFormat::Json,
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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
            security: vec![crate::oas::models::SecurityRequirementGroup {
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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
        let group = crate::oas::models::SecurityRequirementGroup {
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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
        info.security = vec![crate::oas::models::SecurityRequirementGroup {
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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
            security: vec![crate::oas::models::SecurityRequirementGroup {
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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
            security: vec![crate::oas::models::SecurityRequirementGroup::anonymous()],
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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
                    flows: crate::oas::models::OAuthFlows {
                        implicit: None,
                        password: None,
                        client_credentials: None,
                        authorization_code: None,
                        device_authorization: Some(crate::oas::models::OAuthFlow {
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
                    },
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
            security: vec![crate::oas::models::SecurityRequirementGroup {
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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
            security: vec![crate::oas::models::SecurityRequirementGroup {
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
            security: vec![crate::oas::models::SecurityRequirementGroup {
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
            .unwrap_err();
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

        let doc =
            generate_openapi_document_with_routes(&[], &[route_a, route_b], None, &info).unwrap();
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

        let doc =
            generate_openapi_document_with_routes(&[], &[route_a, route_b], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc =
            generate_openapi_document_with_routes(&[], &[webhook_route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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
            security: vec![crate::oas::models::SecurityRequirementGroup {
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
        let responses = &doc["paths"]["/items"]["get"]["responses"];
        assert_eq!(responses["404"]["description"], "Not Found");
        assert_eq!(
            responses["200"]["content"]["application/json"]["schema"]["type"],
            "integer"
        );
    }

    #[test]
    fn test_generate_openapi_document_rehydrates_header_content() {
        let info = OpenApiInfo::new("Test API", "1.0.0");
        let raw_responses = json!({
            "200": {
                "description": "OK",
                "headers": {
                    "X-Token": {
                        "schema": { "type": "string" },
                        "x-cdd-content": {
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
        let header = &doc["paths"]["/items"]["get"]["responses"]["200"]["headers"]["X-Token"];
        assert!(header.get("content").is_some());
        assert!(header.get("x-cdd-content").is_none());
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
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

        let doc = generate_openapi_document_with_routes(&[], &[route], None, &info).unwrap();
        let enc = &doc["paths"]["/upload"]["post"]["requestBody"]["content"]["multipart/form-data"]
            ["encoding"]["payload"];
        assert_eq!(enc["contentType"], "multipart/mixed");
        assert_eq!(enc["encoding"]["part"]["contentType"], "application/json");
        assert_eq!(enc["prefixEncoding"][0]["contentType"], "text/plain");
        assert_eq!(
            enc["itemEncoding"]["contentType"],
            "application/octet-stream"
        );
    }
}
