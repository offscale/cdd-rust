#![deny(missing_docs)]

//! # OpenAPI Validation
//!
//! Helper functions that enforce structural requirements from the OpenAPI 3.2.0
//! specification before deeper parsing or code generation.
//!
//! Additional validations include:
//! - Tag parent references must resolve to existing tags.
//! - Tag parent chains must be acyclic.
//! - Server variable enums, when present, must be non-empty.
//! - Path Item `$ref` must not be combined with sibling fields.
//! - Responses must define at least one response entry.

use crate::error::{AppError, AppResult};
use crate::oas::ref_utils::extract_component_name;
use crate::oas::routes::shims::ShimRequestBody;
use crate::oas::routes::shims::{
    ShimComponents, ShimExternalDocs, ShimOpenApi, ShimPathItem, ShimSecurityScheme, ShimServer,
};
use regex::Regex;
use std::collections::{BTreeMap, HashMap, HashSet};
use url::Url;
use utoipa::openapi::RefOr;

const COMPONENT_KEY_PATTERN: &str = r"^[a-zA-Z0-9._-]+$";
const COMPONENT_SECTIONS: [&str; 11] = [
    "schemas",
    "responses",
    "parameters",
    "examples",
    "requestBodies",
    "headers",
    "securitySchemes",
    "links",
    "callbacks",
    "pathItems",
    "mediaTypes",
];

/// Validates required root-level fields for an OpenAPI document.
pub(crate) fn validate_openapi_root(openapi: &ShimOpenApi) -> AppResult<()> {
    if openapi.info.is_none() {
        return Err(AppError::General(
            "OpenAPI document missing required 'info' object".into(),
        ));
    }

    if openapi.openapi.is_some() {
        if let Some(self_uri) = &openapi.self_uri {
            validate_uri_reference(self_uri, "$self")?;
        }
        if let Some(dialect) = &openapi.json_schema_dialect {
            validate_uri_reference(dialect, "jsonSchemaDialect")?;
        }
    }

    // OAS 3.x requires at least one of components, paths, or webhooks.
    if openapi.openapi.is_some()
        && openapi.components.is_none()
        && openapi.paths.is_empty()
        && openapi.webhooks.is_none()
    {
        return Err(AppError::General(
            "OpenAPI document must define at least one of 'components', 'paths', or 'webhooks'"
                .into(),
        ));
    }

    validate_info_fields(openapi)?;
    validate_external_docs(openapi)?;
    validate_tags_unique(openapi)?;
    validate_tag_hierarchy(openapi)?;
    validate_servers(openapi)?;
    validate_security_schemes(openapi)?;
    validate_security_requirements(openapi)?;
    validate_path_item_refs(openapi)?;
    validate_responses(openapi)?;
    validate_request_bodies(openapi)?;
    validate_headers(openapi)?;

    Ok(())
}

/// Validates that tag names are unique when tags are provided.
pub(crate) fn validate_tags_unique(openapi: &ShimOpenApi) -> AppResult<()> {
    let Some(tags) = &openapi.tags else {
        return Ok(());
    };

    let mut seen = HashSet::new();
    for tag in tags {
        if !seen.insert(tag.name.clone()) {
            return Err(AppError::General(format!(
                "Duplicate tag name '{}' detected",
                tag.name
            )));
        }
    }

    Ok(())
}

/// Validates tag parent references and prevents circular tag hierarchies.
pub(crate) fn validate_tag_hierarchy(openapi: &ShimOpenApi) -> AppResult<()> {
    let Some(tags) = &openapi.tags else {
        return Ok(());
    };

    let mut parents: HashMap<String, Option<String>> = HashMap::new();
    for tag in tags {
        parents.insert(tag.name.clone(), tag.parent.clone());
    }

    for tag in tags {
        if let Some(parent) = &tag.parent {
            if !parents.contains_key(parent) {
                return Err(AppError::General(format!(
                    "Tag '{}' references missing parent tag '{}'",
                    tag.name, parent
                )));
            }
        }
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for tag in tags {
        validate_tag_chain(&tag.name, &parents, &mut visiting, &mut visited)?;
    }

    Ok(())
}

fn validate_tag_chain(
    tag: &str,
    parents: &HashMap<String, Option<String>>,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
) -> AppResult<()> {
    if visited.contains(tag) {
        return Ok(());
    }
    if !visiting.insert(tag.to_string()) {
        return Err(AppError::General(format!(
            "Tag hierarchy contains a cycle at '{}'",
            tag
        )));
    }

    if let Some(Some(parent)) = parents.get(tag) {
        validate_tag_chain(parent, parents, visiting, visited)?;
    }

    visiting.remove(tag);
    visited.insert(tag.to_string());
    Ok(())
}

/// Validates Server Objects for URL correctness and variable rules.
pub(crate) fn validate_servers(openapi: &ShimOpenApi) -> AppResult<()> {
    // Swagger 2.0 does not define `servers`.
    if openapi.openapi.is_none() {
        return Ok(());
    }

    if let Some(servers) = &openapi.servers {
        validate_server_list(servers, "servers")?;
    }

    for (path, path_item) in &openapi.paths {
        validate_path_item_servers(path_item, &format!("paths.{}", path))?;
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in webhooks {
            if let utoipa::openapi::RefOr::T(path_item) = item_or_ref {
                validate_path_item_servers(path_item, &format!("webhooks.{}", name))?;
            }
        }
    }

    Ok(())
}

fn validate_info_fields(openapi: &ShimOpenApi) -> AppResult<()> {
    let Some(info) = &openapi.info else {
        return Ok(());
    };

    if info.title.trim().is_empty() {
        return Err(AppError::General(
            "Info.title must be a non-empty string".into(),
        ));
    }

    if info.version.trim().is_empty() {
        return Err(AppError::General(
            "Info.version must be a non-empty string".into(),
        ));
    }

    if let Some(terms) = &info.terms_of_service {
        validate_uri_reference(terms, "info.termsOfService")?;
    }

    if let Some(contact) = &info.contact {
        validate_contact(contact)?;
    }

    if let Some(license) = &info.license {
        validate_license(license)?;
    }

    Ok(())
}

fn validate_contact(contact: &crate::oas::routes::shims::ShimContact) -> AppResult<()> {
    if let Some(url) = &contact.url {
        validate_uri_reference(url, "info.contact.url")?;
    }

    if let Some(email) = &contact.email {
        if !is_valid_email(email) {
            return Err(AppError::General(format!(
                "info.contact.email '{}' is not a valid email address",
                email
            )));
        }
    }

    Ok(())
}

fn validate_license(license: &crate::oas::routes::shims::ShimLicense) -> AppResult<()> {
    if license.name.trim().is_empty() {
        return Err(AppError::General(
            "info.license.name must be a non-empty string".into(),
        ));
    }

    if license.identifier.is_some() && license.url.is_some() {
        return Err(AppError::General(
            "info.license cannot specify both 'identifier' and 'url'".into(),
        ));
    }

    if let Some(url) = &license.url {
        validate_uri_reference(url, "info.license.url")?;
    }

    Ok(())
}

fn validate_external_docs(openapi: &ShimOpenApi) -> AppResult<()> {
    let components = openapi.components.as_ref();
    if let Some(external) = &openapi.external_docs {
        validate_external_docs_reference("externalDocs.url", external)?;
    }

    if let Some(tags) = &openapi.tags {
        for tag in tags {
            if let Some(external) = &tag.external_docs {
                let context = format!("tags.{}.externalDocs.url", tag.name);
                validate_external_docs_reference(&context, external)?;
            }
        }
    }

    for (path, path_item) in &openapi.paths {
        validate_path_item_external_docs(path_item, &format!("paths.{}", path), components)?;
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in webhooks {
            if let utoipa::openapi::RefOr::T(path_item) = item_or_ref {
                validate_path_item_external_docs(
                    path_item,
                    &format!("webhooks.{}", name),
                    components,
                )?;
            }
        }
    }

    Ok(())
}

fn validate_path_item_external_docs(
    path_item: &ShimPathItem,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    let validate_op = |label: &str, op: &Option<crate::oas::routes::shims::ShimOperation>| {
        if let Some(op) = op {
            if let Some(external) = &op.external_docs {
                let ctx = format!("{}.{}.externalDocs.url", context, label);
                validate_external_docs_reference(&ctx, external)?;
            }
            if let Some(callbacks) = &op.callbacks {
                let cb_ctx = format!("{}.{}.callbacks", context, label);
                validate_callbacks_external_docs(callbacks, &cb_ctx, components)?;
            }
        }
        Ok::<_, AppError>(())
    };

    validate_op("get", &path_item.get)?;
    validate_op("post", &path_item.post)?;
    validate_op("put", &path_item.put)?;
    validate_op("delete", &path_item.delete)?;
    validate_op("patch", &path_item.patch)?;
    validate_op("options", &path_item.options)?;
    validate_op("head", &path_item.head)?;
    validate_op("trace", &path_item.trace)?;
    validate_op("query", &path_item.query)?;

    if let Some(additional) = &path_item.additional_operations {
        for (method, op) in additional {
            if let Some(external) = &op.external_docs {
                let ctx = format!("{}.{}.externalDocs.url", context, method);
                validate_external_docs_reference(&ctx, external)?;
            }
            if let Some(callbacks) = &op.callbacks {
                let cb_ctx = format!("{}.{}.callbacks", context, method);
                validate_callbacks_external_docs(callbacks, &cb_ctx, components)?;
            }
        }
    }

    Ok(())
}

fn validate_callbacks_external_docs(
    callbacks: &std::collections::BTreeMap<
        String,
        RefOr<std::collections::BTreeMap<String, ShimPathItem>>,
    >,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    for (name, cb_ref) in callbacks {
        let cb_map = crate::oas::routes::callbacks::resolve_callback_object(cb_ref, components)?;
        for (expr, path_item) in cb_map {
            let cb_ctx = format!("{}.{}.{}", context, name, expr);
            validate_path_item_external_docs(&path_item, &cb_ctx, components)?;
        }
    }
    Ok(())
}

fn validate_external_docs_reference(context: &str, external: &ShimExternalDocs) -> AppResult<()> {
    validate_uri_reference(&external.url, context)
}

fn validate_security_schemes(openapi: &ShimOpenApi) -> AppResult<()> {
    let is_oas3 = openapi.openapi.is_some();

    if let Some(components) = &openapi.components {
        if let Some(schemes) = &components.security_schemes {
            for (name, scheme_ref) in schemes {
                match scheme_ref {
                    utoipa::openapi::RefOr::T(scheme) => {
                        validate_security_scheme(name, scheme, is_oas3)?;
                    }
                    utoipa::openapi::RefOr::Ref(r) => {
                        if let Some(ref_name) = crate::oas::ref_utils::extract_component_name(
                            &r.ref_location,
                            openapi.self_uri.as_deref(),
                            "securitySchemes",
                        ) {
                            if let Some(utoipa::openapi::RefOr::T(resolved)) =
                                schemes.get(&ref_name)
                            {
                                validate_security_scheme(&ref_name, resolved, is_oas3)?;
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(definitions) = &openapi.security_definitions {
        for (name, scheme) in definitions {
            validate_security_scheme(name, scheme, false)?;
        }
    }

    Ok(())
}

fn validate_security_scheme(
    name: &str,
    scheme: &ShimSecurityScheme,
    is_oas3: bool,
) -> AppResult<()> {
    match scheme {
        ShimSecurityScheme::ApiKey(key) => {
            if key.name.trim().is_empty() {
                return Err(AppError::General(format!(
                    "Security scheme '{}' apiKey name must be non-empty",
                    name
                )));
            }
            match key.in_loc.as_str() {
                "query" | "header" | "cookie" => {}
                _ => {
                    return Err(AppError::General(format!(
                        "Security scheme '{}' apiKey 'in' must be query, header, or cookie",
                        name
                    )));
                }
            }
        }
        ShimSecurityScheme::Http(http) => {
            if http.scheme.trim().is_empty() {
                return Err(AppError::General(format!(
                    "Security scheme '{}' http scheme must be non-empty",
                    name
                )));
            }
        }
        ShimSecurityScheme::OAuth2(oauth) => {
            if is_oas3 && oauth.flows.is_none() {
                return Err(AppError::General(format!(
                    "Security scheme '{}' oauth2 must define 'flows'",
                    name
                )));
            }

            if let Some(meta_url) = &oauth.oauth2_metadata_url {
                validate_https_url(
                    meta_url,
                    &format!("securitySchemes.{}.oauth2MetadataUrl", name),
                )?;
            }

            if let Some(flows) = &oauth.flows {
                validate_oauth_flows(name, flows)?;
            } else if !is_oas3 {
                validate_oauth2_legacy(name, oauth)?;
            }
        }
        ShimSecurityScheme::OpenIdConnect(oidc) => {
            validate_https_url(
                &oidc.open_id_connect_url,
                &format!("securitySchemes.{}.openIdConnectUrl", name),
            )?;
        }
        ShimSecurityScheme::MutualTls(_) => {}
        ShimSecurityScheme::Basic => {
            if is_oas3 {
                return Err(AppError::General(format!(
                    "Security scheme '{}' uses legacy 'basic' type in OAS 3.x",
                    name
                )));
            }
        }
    }

    Ok(())
}

fn validate_oauth_flows(
    name: &str,
    flows: &crate::oas::routes::shims::ShimOAuthFlows,
) -> AppResult<()> {
    if let Some(flow) = &flows.implicit {
        validate_oauth_flow(name, "implicit", flow, true, false, false)?;
    }
    if let Some(flow) = &flows.password {
        validate_oauth_flow(name, "password", flow, false, true, false)?;
    }
    if let Some(flow) = &flows.client_credentials {
        validate_oauth_flow(name, "clientCredentials", flow, false, true, false)?;
    }
    if let Some(flow) = &flows.authorization_code {
        validate_oauth_flow(name, "authorizationCode", flow, true, true, false)?;
    }
    if let Some(flow) = &flows.device_authorization {
        validate_oauth_flow(name, "deviceAuthorization", flow, false, true, true)?;
    }

    Ok(())
}

fn validate_oauth_flow(
    scheme_name: &str,
    flow_name: &str,
    flow: &crate::oas::routes::shims::ShimOAuthFlow,
    require_auth_url: bool,
    require_token_url: bool,
    require_device_url: bool,
) -> AppResult<()> {
    if require_auth_url {
        let url = flow.authorization_url.as_deref().ok_or_else(|| {
            AppError::General(format!(
                "Security scheme '{}' oauth2 flow '{}' missing authorizationUrl",
                scheme_name, flow_name
            ))
        })?;
        validate_https_url(
            url,
            &format!(
                "securitySchemes.{}.flows.{}.authorizationUrl",
                scheme_name, flow_name
            ),
        )?;
    }

    if require_token_url {
        let url = flow.token_url.as_deref().ok_or_else(|| {
            AppError::General(format!(
                "Security scheme '{}' oauth2 flow '{}' missing tokenUrl",
                scheme_name, flow_name
            ))
        })?;
        validate_https_url(
            url,
            &format!(
                "securitySchemes.{}.flows.{}.tokenUrl",
                scheme_name, flow_name
            ),
        )?;
    }

    if require_device_url {
        let url = flow.device_authorization_url.as_deref().ok_or_else(|| {
            AppError::General(format!(
                "Security scheme '{}' oauth2 flow '{}' missing deviceAuthorizationUrl",
                scheme_name, flow_name
            ))
        })?;
        validate_https_url(
            url,
            &format!(
                "securitySchemes.{}.flows.{}.deviceAuthorizationUrl",
                scheme_name, flow_name
            ),
        )?;
    }

    if let Some(refresh) = &flow.refresh_url {
        validate_https_url(
            refresh,
            &format!(
                "securitySchemes.{}.flows.{}.refreshUrl",
                scheme_name, flow_name
            ),
        )?;
    }

    Ok(())
}

fn validate_oauth2_legacy(
    scheme_name: &str,
    oauth: &crate::oas::routes::shims::ShimOAuth2,
) -> AppResult<()> {
    let Some(flow) = oauth.flow.as_deref() else {
        return Err(AppError::General(format!(
            "Security scheme '{}' oauth2 legacy flow missing 'flow' field",
            scheme_name
        )));
    };

    match flow {
        "implicit" => {
            let url = oauth.authorization_url.as_deref().ok_or_else(|| {
                AppError::General(format!(
                    "Security scheme '{}' oauth2 implicit missing authorizationUrl",
                    scheme_name
                ))
            })?;
            validate_https_url(
                url,
                &format!("securityDefinitions.{}.authorizationUrl", scheme_name),
            )?;
        }
        "password" | "application" => {
            let url = oauth.token_url.as_deref().ok_or_else(|| {
                AppError::General(format!(
                    "Security scheme '{}' oauth2 {} missing tokenUrl",
                    scheme_name, flow
                ))
            })?;
            validate_https_url(
                url,
                &format!("securityDefinitions.{}.tokenUrl", scheme_name),
            )?;
        }
        "accessCode" => {
            let auth_url = oauth.authorization_url.as_deref().ok_or_else(|| {
                AppError::General(format!(
                    "Security scheme '{}' oauth2 accessCode missing authorizationUrl",
                    scheme_name
                ))
            })?;
            let token_url = oauth.token_url.as_deref().ok_or_else(|| {
                AppError::General(format!(
                    "Security scheme '{}' oauth2 accessCode missing tokenUrl",
                    scheme_name
                ))
            })?;
            validate_https_url(
                auth_url,
                &format!("securityDefinitions.{}.authorizationUrl", scheme_name),
            )?;
            validate_https_url(
                token_url,
                &format!("securityDefinitions.{}.tokenUrl", scheme_name),
            )?;
        }
        _ => {
            return Err(AppError::General(format!(
                "Security scheme '{}' oauth2 legacy flow '{}' is not supported",
                scheme_name, flow
            )));
        }
    }

    Ok(())
}

fn validate_security_requirements(openapi: &ShimOpenApi) -> AppResult<()> {
    let mut known = HashSet::new();
    if let Some(components) = &openapi.components {
        if let Some(schemes) = &components.security_schemes {
            known.extend(schemes.keys().cloned());
        }
    }
    if let Some(definitions) = &openapi.security_definitions {
        known.extend(definitions.keys().cloned());
    }

    if let Some(global) = &openapi.security {
        validate_security_requirement_list(global, &known, "security")?;
    }

    for (path, path_item) in &openapi.paths {
        validate_path_item_security(path_item, &format!("paths.{}", path), &known)?;
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in webhooks {
            if let utoipa::openapi::RefOr::T(path_item) = item_or_ref {
                validate_path_item_security(path_item, &format!("webhooks.{}", name), &known)?;
            }
        }
    }

    Ok(())
}

fn validate_path_item_security(
    path_item: &ShimPathItem,
    context: &str,
    known_schemes: &HashSet<String>,
) -> AppResult<()> {
    let validate_op = |label: &str, op: &Option<crate::oas::routes::shims::ShimOperation>| {
        if let Some(op) = op {
            if let Some(security) = &op.security {
                let ctx = format!("{}.{}.security", context, label);
                validate_security_requirement_list(security, known_schemes, &ctx)?;
            }
        }
        Ok::<_, AppError>(())
    };

    validate_op("get", &path_item.get)?;
    validate_op("post", &path_item.post)?;
    validate_op("put", &path_item.put)?;
    validate_op("delete", &path_item.delete)?;
    validate_op("patch", &path_item.patch)?;
    validate_op("options", &path_item.options)?;
    validate_op("head", &path_item.head)?;
    validate_op("trace", &path_item.trace)?;
    validate_op("query", &path_item.query)?;

    if let Some(additional) = &path_item.additional_operations {
        for (method, op) in additional {
            if let Some(security) = &op.security {
                let ctx = format!("{}.{}.security", context, method);
                validate_security_requirement_list(security, known_schemes, &ctx)?;
            }
        }
    }

    Ok(())
}

fn validate_security_requirement_list(
    requirements: &[serde_json::Value],
    known: &HashSet<String>,
    context: &str,
) -> AppResult<()> {
    for req in requirements {
        let Ok(map) = serde_json::from_value::<HashMap<String, Vec<String>>>(req.clone()) else {
            return Err(AppError::General(format!(
                "Security requirement in {} must be an object",
                context
            )));
        };

        if map.is_empty() {
            continue;
        }

        for scheme_name in map.keys() {
            if known.contains(scheme_name) {
                continue;
            }
            if looks_like_uri_reference(scheme_name) && is_valid_uri_reference(scheme_name) {
                continue;
            }
            return Err(AppError::General(format!(
                "Security requirement '{}' in {} does not match a known security scheme",
                scheme_name, context
            )));
        }
    }

    Ok(())
}

fn validate_uri_reference(value: &str, context: &str) -> AppResult<()> {
    if is_valid_uri_reference(value) {
        return Ok(());
    }

    Err(AppError::General(format!(
        "{} '{}' is not a valid URI reference",
        context, value
    )))
}

fn validate_https_url(value: &str, context: &str) -> AppResult<()> {
    let parsed = Url::parse(value).map_err(|_| {
        AppError::General(format!(
            "{} '{}' must be an absolute HTTPS URL",
            context, value
        ))
    })?;

    if parsed.scheme() != "https" {
        return Err(AppError::General(format!(
            "{} '{}' must use https scheme",
            context, value
        )));
    }

    Ok(())
}

fn is_valid_uri_reference(value: &str) -> bool {
    if value.trim().is_empty() {
        return false;
    }
    if value.chars().any(|c| c.is_whitespace()) {
        return false;
    }

    if Url::parse(value).is_ok() {
        return true;
    }

    let base = Url::parse("https://example.com").expect("valid base url");
    Url::options().base_url(Some(&base)).parse(value).is_ok()
}

fn looks_like_uri_reference(value: &str) -> bool {
    value.contains(':')
        || value.starts_with('/')
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with('#')
}

fn is_valid_email(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.contains(' ') {
        return false;
    }
    let mut parts = trimmed.split('@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    if local.is_empty() || domain.is_empty() || parts.next().is_some() {
        return false;
    }
    domain.contains('.')
}

fn validate_path_item_servers(path_item: &ShimPathItem, context: &str) -> AppResult<()> {
    if let Some(servers) = &path_item.servers {
        validate_server_list(servers, &format!("{}.servers", context))?;
    }

    let validate_op = |label: &str, op: &Option<crate::oas::routes::shims::ShimOperation>| {
        if let Some(op) = op {
            if let Some(servers) = &op.servers {
                validate_server_list(servers, &format!("{}.{}.servers", context, label))?;
            }
        }
        Ok::<_, AppError>(())
    };

    validate_op("get", &path_item.get)?;
    validate_op("post", &path_item.post)?;
    validate_op("put", &path_item.put)?;
    validate_op("delete", &path_item.delete)?;
    validate_op("patch", &path_item.patch)?;
    validate_op("options", &path_item.options)?;
    validate_op("head", &path_item.head)?;
    validate_op("trace", &path_item.trace)?;
    validate_op("query", &path_item.query)?;

    if let Some(additional) = &path_item.additional_operations {
        for (method, op) in additional {
            if let Some(servers) = &op.servers {
                validate_server_list(servers, &format!("{}.{}.servers", context, method))?;
            }
        }
    }

    Ok(())
}

fn validate_responses(openapi: &ShimOpenApi) -> AppResult<()> {
    let components = openapi.components.as_ref();
    for (path, path_item) in &openapi.paths {
        validate_path_item_responses(path_item, &format!("paths.{}", path), components)?;
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in webhooks {
            if let utoipa::openapi::RefOr::T(path_item) = item_or_ref {
                validate_path_item_responses(path_item, &format!("webhooks.{}", name), components)?;
            }
        }
    }

    Ok(())
}

fn validate_request_bodies(openapi: &ShimOpenApi) -> AppResult<()> {
    if openapi.openapi.is_none() {
        return Ok(());
    }

    let components = openapi.components.as_ref();
    for (path, path_item) in &openapi.paths {
        validate_path_item_request_bodies(path_item, &format!("paths.{}", path), components)?;
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in webhooks {
            if let utoipa::openapi::RefOr::T(path_item) = item_or_ref {
                validate_path_item_request_bodies(
                    path_item,
                    &format!("webhooks.{}", name),
                    components,
                )?;
            }
        }
    }

    if let Some(components) = openapi.components.as_ref() {
        if let Some(values) = components.extra.get("requestBodies") {
            if let Some(map) = values.as_object() {
                for (name, body_val) in map {
                    let parsed = serde_json::from_value::<utoipa::openapi::RefOr<ShimRequestBody>>(
                        body_val.clone(),
                    );
                    if let Ok(body) = parsed {
                        let ctx = format!("components.requestBodies.{}", name);
                        validate_request_body_ref_or(&body, &ctx, Some(components))?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn validate_path_item_request_bodies(
    path_item: &ShimPathItem,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    let validate_op = |label: &str, op: &Option<crate::oas::routes::shims::ShimOperation>| {
        if let Some(op) = op {
            if let Some(body) = &op.request_body {
                validate_request_body_ref_or(
                    body,
                    &format!("{}.{}.requestBody", context, label),
                    components,
                )?;
            }
        }
        Ok::<_, AppError>(())
    };

    validate_op("get", &path_item.get)?;
    validate_op("post", &path_item.post)?;
    validate_op("put", &path_item.put)?;
    validate_op("delete", &path_item.delete)?;
    validate_op("patch", &path_item.patch)?;
    validate_op("options", &path_item.options)?;
    validate_op("head", &path_item.head)?;
    validate_op("trace", &path_item.trace)?;
    validate_op("query", &path_item.query)?;

    if let Some(additional) = &path_item.additional_operations {
        for (method, op) in additional {
            if let Some(body) = &op.request_body {
                validate_request_body_ref_or(
                    body,
                    &format!("{}.{}.requestBody", context, method),
                    components,
                )?;
            }
        }
    }

    Ok(())
}

fn validate_request_body_ref_or(
    body: &utoipa::openapi::RefOr<ShimRequestBody>,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    match body {
        utoipa::openapi::RefOr::T(b) => validate_request_body_content(b, context),
        utoipa::openapi::RefOr::Ref(r) => {
            if let Some(resolved) = resolve_request_body_from_components(
                &r.ref_location,
                components,
                &mut HashSet::new(),
            ) {
                validate_request_body_content(&resolved, context)?;
            }
            Ok(())
        }
    }
}

fn validate_request_body_content(body: &ShimRequestBody, context: &str) -> AppResult<()> {
    if body.inner.content.is_empty() {
        return Err(AppError::General(format!(
            "{} must define at least one media type in content",
            context
        )));
    }
    if let Some(content) = body.raw.get("content") {
        validate_media_type_examples(content, &format!("{}.content", context))?;
    }
    Ok(())
}

fn validate_headers(openapi: &ShimOpenApi) -> AppResult<()> {
    // Swagger 2.0 does not define Header Objects with OAS 3.x rules.
    if openapi.openapi.is_none() {
        return Ok(());
    }

    let components = openapi.components.as_ref();

    if let Some(comps) = components {
        if let Some(headers_val) = comps.extra.get("headers") {
            if let Some(headers) = headers_val.as_object() {
                validate_header_entries(headers, "components.headers", components)?;
            }
        }
    }

    for (path, path_item) in &openapi.paths {
        validate_path_item_headers(path_item, &format!("paths.{}", path), components)?;
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in webhooks {
            if let utoipa::openapi::RefOr::T(path_item) = item_or_ref {
                validate_path_item_headers(path_item, &format!("webhooks.{}", name), components)?;
            }
        }
    }

    Ok(())
}

fn validate_path_item_headers(
    path_item: &ShimPathItem,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    let validate_op = |label: &str, op: &Option<crate::oas::routes::shims::ShimOperation>| {
        if let Some(op) = op {
            validate_response_headers(
                &op.responses,
                &format!("{}.{}.responses", context, label),
                components,
            )?;
        }
        Ok::<_, AppError>(())
    };

    validate_op("get", &path_item.get)?;
    validate_op("post", &path_item.post)?;
    validate_op("put", &path_item.put)?;
    validate_op("delete", &path_item.delete)?;
    validate_op("patch", &path_item.patch)?;
    validate_op("options", &path_item.options)?;
    validate_op("head", &path_item.head)?;
    validate_op("trace", &path_item.trace)?;
    validate_op("query", &path_item.query)?;

    if let Some(additional) = &path_item.additional_operations {
        for (method, op) in additional {
            validate_response_headers(
                &op.responses,
                &format!("{}.{}.responses", context, method),
                components,
            )?;
        }
    }

    Ok(())
}

fn validate_response_headers(
    responses: &crate::oas::routes::shims::ShimResponses,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    for (code, resp) in &responses.inner.responses {
        let resp_context = format!("{}.{}", context, code);
        let raw_response = match resp {
            utoipa::openapi::RefOr::T(_) => responses.raw.get(code).cloned(),
            utoipa::openapi::RefOr::Ref(r) => resolve_response_raw_from_components(
                &r.ref_location,
                components,
                &mut HashSet::new(),
            ),
        };

        let Some(raw) = raw_response else {
            continue;
        };
        let Some(headers) = raw.get("headers").and_then(|h| h.as_object()) else {
            continue;
        };
        validate_header_entries(headers, &format!("{}.headers", resp_context), components)?;
    }
    Ok(())
}

fn validate_header_entries(
    headers: &serde_json::Map<String, serde_json::Value>,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    for (name, header_val) in headers {
        if name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        let mut visited = HashSet::new();
        validate_header_value(
            header_val,
            &format!("{}.{}", context, name),
            components,
            &mut visited,
        )?;
    }
    Ok(())
}

fn validate_header_value(
    value: &serde_json::Value,
    context: &str,
    components: Option<&ShimComponents>,
    visited: &mut HashSet<String>,
) -> AppResult<()> {
    let Some(obj) = value.as_object() else {
        return Err(AppError::General(format!(
            "Header object in {} must be an object or $ref",
            context
        )));
    };

    if let Some(ref_str) = obj.get("$ref").and_then(|v| v.as_str()) {
        if let Some(resolved) = resolve_header_raw_from_components(ref_str, components, visited) {
            return validate_header_value(&resolved, context, components, visited);
        }
        return Ok(());
    }

    if obj.contains_key("name") || obj.contains_key("in") {
        return Err(AppError::General(format!(
            "Header object in {} must not define 'name' or 'in'",
            context
        )));
    }

    if obj.contains_key("allowEmptyValue") {
        return Err(AppError::General(format!(
            "Header object in {} must not define allowEmptyValue",
            context
        )));
    }

    if let Some(style) = obj.get("style").and_then(|v| v.as_str()) {
        if style != "simple" {
            return Err(AppError::General(format!(
                "Header object in {} uses style '{}' which is not allowed for headers",
                context, style
            )));
        }
    }

    let has_schema = obj.contains_key("schema");
    let has_content = obj.contains_key("content");
    if has_schema && has_content {
        return Err(AppError::General(format!(
            "Header object in {} must not define both schema and content",
            context
        )));
    }
    if !has_schema && !has_content {
        return Err(AppError::General(format!(
            "Header object in {} must define either schema or content",
            context
        )));
    }

    if let Some(content) = obj.get("content") {
        let Some(content_map) = content.as_object() else {
            return Err(AppError::General(format!(
                "Header object in {} content must be an object",
                context
            )));
        };
        if content_map.len() != 1 {
            return Err(AppError::General(format!(
                "Header object in {} must define exactly one media type in content",
                context
            )));
        }
        validate_media_type_examples(content, &format!("{}.content", context))?;
    }

    Ok(())
}

fn validate_media_type_examples(content: &serde_json::Value, context: &str) -> AppResult<()> {
    let Some(map) = content.as_object() else {
        return Ok(());
    };
    for (media_type, media_obj) in map {
        let Some(obj) = media_obj.as_object() else {
            continue;
        };
        if obj.contains_key("example") && obj.contains_key("examples") {
            return Err(AppError::General(format!(
                "{}.{} must not define both example and examples",
                context, media_type
            )));
        }
    }
    Ok(())
}

fn resolve_header_raw_from_components(
    ref_str: &str,
    components: Option<&ShimComponents>,
    visited: &mut HashSet<String>,
) -> Option<serde_json::Value> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let name = extract_component_name(ref_str, self_uri, "headers")?;
    if !visited.insert(name.clone()) {
        return None;
    }
    let header_json = comps.extra.get("headers").and_then(|h| h.get(&name))?;
    let obj = header_json.as_object()?;
    if let Some(next_ref) = obj.get("$ref").and_then(|v| v.as_str()) {
        return resolve_header_raw_from_components(next_ref, components, visited);
    }
    Some(header_json.clone())
}

fn resolve_request_body_from_components(
    ref_str: &str,
    components: Option<&ShimComponents>,
    visited: &mut HashSet<String>,
) -> Option<ShimRequestBody> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let name = extract_component_name(ref_str, self_uri, "requestBodies")?;
    if !visited.insert(name.clone()) {
        return None;
    }

    let body_json = comps
        .extra
        .get("requestBodies")
        .and_then(|b| b.get(&name))?;
    let parsed: utoipa::openapi::RefOr<ShimRequestBody> =
        serde_json::from_value(body_json.clone()).ok()?;

    match parsed {
        utoipa::openapi::RefOr::T(body) => Some(body),
        utoipa::openapi::RefOr::Ref(r) => {
            resolve_request_body_from_components(&r.ref_location, components, visited)
        }
    }
}

fn validate_path_item_refs(openapi: &ShimOpenApi) -> AppResult<()> {
    for (path, path_item) in &openapi.paths {
        validate_path_item_ref(path_item, &format!("paths.{}", path))?;
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in webhooks {
            if let utoipa::openapi::RefOr::T(path_item) = item_or_ref {
                validate_path_item_ref(path_item, &format!("webhooks.{}", name))?;
            }
        }
    }

    if let Some(components) = &openapi.components {
        if let Some(path_items) = &components.path_items {
            for (name, ref_or) in path_items {
                if let utoipa::openapi::RefOr::T(path_item) = ref_or {
                    validate_path_item_ref(path_item, &format!("components.pathItems.{}", name))?;
                }
            }
        }
    }

    Ok(())
}

fn validate_path_item_ref(path_item: &ShimPathItem, context: &str) -> AppResult<()> {
    if path_item.ref_path.is_none() {
        return Ok(());
    }

    let has_siblings = path_item.summary.is_some()
        || path_item.description.is_some()
        || path_item.servers.as_ref().map_or(false, |s| !s.is_empty())
        || path_item
            .parameters
            .as_ref()
            .map_or(false, |p| !p.is_empty())
        || path_item.get.is_some()
        || path_item.post.is_some()
        || path_item.put.is_some()
        || path_item.delete.is_some()
        || path_item.patch.is_some()
        || path_item.options.is_some()
        || path_item.head.is_some()
        || path_item.trace.is_some()
        || path_item.query.is_some()
        || path_item
            .additional_operations
            .as_ref()
            .map_or(false, |ops| !ops.is_empty());

    if has_siblings {
        return Err(AppError::General(format!(
            "{} defines $ref alongside other Path Item fields which is not allowed",
            context
        )));
    }

    Ok(())
}

fn validate_path_item_responses(
    path_item: &ShimPathItem,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    let validate_op = |label: &str, op: &Option<crate::oas::routes::shims::ShimOperation>| {
        if let Some(op) = op {
            validate_response_map(
                &op.responses,
                &format!("{}.{}.responses", context, label),
                components,
            )?;
        }
        Ok::<_, AppError>(())
    };

    validate_op("get", &path_item.get)?;
    validate_op("post", &path_item.post)?;
    validate_op("put", &path_item.put)?;
    validate_op("delete", &path_item.delete)?;
    validate_op("patch", &path_item.patch)?;
    validate_op("options", &path_item.options)?;
    validate_op("head", &path_item.head)?;
    validate_op("trace", &path_item.trace)?;
    validate_op("query", &path_item.query)?;

    if let Some(additional) = &path_item.additional_operations {
        for (method, op) in additional {
            validate_response_map(
                &op.responses,
                &format!("{}.{}.responses", context, method),
                components,
            )?;
        }
    }

    Ok(())
}

fn validate_response_map(
    responses: &crate::oas::routes::shims::ShimResponses,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    let status_re = Regex::new(r"^[1-5][0-9]{2}$").expect("Invalid regex constant");
    let range_re = Regex::new(r"^[1-5][xX]{2}$").expect("Invalid regex constant");

    if responses.inner.responses.is_empty() {
        return Err(AppError::General(format!(
            "{} must define at least one response",
            context
        )));
    }

    for key in responses.inner.responses.keys() {
        if key == "default" {
            continue;
        }
        if status_re.is_match(key) || range_re.is_match(key) {
            continue;
        }
        return Err(AppError::General(format!(
            "Response key '{}' in {} must be an HTTP status code or range",
            key, context
        )));
    }

    for (code, resp) in &responses.inner.responses {
        let resp_context = format!("{}.{}", context, code);
        match resp {
            utoipa::openapi::RefOr::T(r) => validate_response_description(r, &resp_context)?,
            utoipa::openapi::RefOr::Ref(r) => {
                if let Some(resolved) = resolve_response_from_components(
                    &r.ref_location,
                    components,
                    &mut HashSet::new(),
                ) {
                    let mut resolved = resolved;
                    if !r.description.is_empty() {
                        resolved.description = r.description.clone();
                    }
                    validate_response_description(&resolved, &resp_context)?;
                }
            }
        }

        let raw_response = match resp {
            utoipa::openapi::RefOr::T(_) => responses.raw.get(code).cloned(),
            utoipa::openapi::RefOr::Ref(r) => resolve_response_raw_from_components(
                &r.ref_location,
                components,
                &mut HashSet::new(),
            ),
        };
        if let Some(raw) = raw_response {
            if let Some(content) = raw.get("content") {
                validate_media_type_examples(content, &format!("{}.content", resp_context))?;
            }
            validate_response_links(&raw, &resp_context, components)?;
        }
    }

    Ok(())
}

fn validate_response_links(
    raw_response: &serde_json::Value,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    let Some(links) = raw_response.get("links") else {
        return Ok(());
    };
    let Some(map) = links.as_object() else {
        return Err(AppError::General(format!(
            "Response links in {} must be an object",
            context
        )));
    };

    let mut visited = HashSet::new();
    let key_re = Regex::new(COMPONENT_KEY_PATTERN).expect("Invalid regex constant");
    for (name, link_val) in map {
        let link_ctx = format!("{}.links.{}", context, name);
        validate_component_key(&key_re, "links", name)?;
        validate_link_value(link_val, &link_ctx, components, &mut visited)?;
    }

    Ok(())
}

fn validate_link_value(
    value: &serde_json::Value,
    context: &str,
    components: Option<&ShimComponents>,
    visited: &mut HashSet<String>,
) -> AppResult<()> {
    let Some(obj) = value.as_object() else {
        return Err(AppError::General(format!(
            "Link object in {} must be an object or $ref",
            context
        )));
    };

    if let Some(ref_str) = obj.get("$ref").and_then(|v| v.as_str()) {
        if !visited.insert(ref_str.to_string()) {
            return Err(AppError::General(format!(
                "Link reference cycle detected at '{}'",
                ref_str
            )));
        }
        let resolved = resolve_link_raw_from_components(ref_str, components).ok_or_else(|| {
            AppError::General(format!(
                "Link reference '{}' could not be resolved",
                ref_str
            ))
        })?;
        validate_link_value(&resolved, context, components, visited)?;
        visited.remove(ref_str);
        return Ok(());
    }

    let has_op_id = obj
        .get("operationId")
        .or_else(|| obj.get("operation_id"))
        .and_then(|v| v.as_str())
        .is_some();
    let has_op_ref = obj
        .get("operationRef")
        .or_else(|| obj.get("operation_ref"))
        .and_then(|v| v.as_str())
        .is_some();
    if has_op_id == has_op_ref {
        return Err(AppError::General(format!(
            "Link in {} must define exactly one of 'operationId' or 'operationRef'",
            context
        )));
    }

    if let Some(server_val) = obj.get("server") {
        let server = serde_json::from_value::<ShimServer>(server_val.clone()).map_err(|e| {
            AppError::General(format!(
                "Link server in {} must be a valid Server Object: {}",
                context, e
            ))
        })?;
        validate_server(&server, &format!("{}.server", context))?;
    }

    Ok(())
}

fn validate_response_description(
    response: &utoipa::openapi::Response,
    context: &str,
) -> AppResult<()> {
    if response.description.trim().is_empty() {
        return Err(AppError::General(format!(
            "Response description in {} must be a non-empty string",
            context
        )));
    }
    Ok(())
}

fn resolve_response_from_components(
    ref_str: &str,
    components: Option<&ShimComponents>,
    visited: &mut HashSet<String>,
) -> Option<utoipa::openapi::Response> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let name = extract_component_name(ref_str, self_uri, "responses")?;
    if !visited.insert(name.clone()) {
        return None;
    }
    let resp_json = comps.extra.get("responses").and_then(|r| r.get(&name))?;
    let parsed: utoipa::openapi::RefOr<utoipa::openapi::Response> =
        serde_json::from_value(resp_json.clone()).ok()?;
    match parsed {
        utoipa::openapi::RefOr::T(resp) => Some(resp),
        utoipa::openapi::RefOr::Ref(r) => {
            resolve_response_from_components(&r.ref_location, components, visited)
        }
    }
}

fn resolve_response_raw_from_components(
    ref_str: &str,
    components: Option<&ShimComponents>,
    visited: &mut HashSet<String>,
) -> Option<serde_json::Value> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let name = extract_component_name(ref_str, self_uri, "responses")?;
    if !visited.insert(name.clone()) {
        return None;
    }
    let resp_json = comps.extra.get("responses").and_then(|r| r.get(&name))?;
    let obj = resp_json.as_object()?;
    if let Some(next_ref) = obj.get("$ref").and_then(|v| v.as_str()) {
        return resolve_response_raw_from_components(next_ref, components, visited);
    }
    Some(resp_json.clone())
}

fn resolve_link_raw_from_components(
    ref_str: &str,
    components: Option<&ShimComponents>,
) -> Option<serde_json::Value> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let name = extract_component_name(ref_str, self_uri, "links")?;
    comps
        .extra
        .get("links")
        .and_then(|links| links.get(&name))
        .cloned()
}

fn validate_server_list(servers: &[ShimServer], context: &str) -> AppResult<()> {
    let mut seen_names = HashSet::new();
    for (idx, server) in servers.iter().enumerate() {
        let server_context = format!("{}[{}]", context, idx);
        if let Some(name) = &server.name {
            if !seen_names.insert(name.clone()) {
                return Err(AppError::General(format!(
                    "Duplicate server name '{}' in {}",
                    name, context
                )));
            }
        }
        validate_server(server, &server_context)?;
    }
    Ok(())
}

fn validate_server(server: &ShimServer, context: &str) -> AppResult<()> {
    validate_server_url(&server.url, context)?;

    let placeholder_re = Regex::new(r"\{([^}]+)}").expect("Invalid regex constant");
    let mut placeholders = Vec::new();
    for cap in placeholder_re.captures_iter(&server.url) {
        placeholders.push(cap[1].to_string());
    }

    match &server.variables {
        Some(vars) => {
            validate_server_variables(&server.url, vars, context)?;
        }
        None => {
            if let Some(name) = placeholders.first() {
                return Err(AppError::General(format!(
                    "Server URL '{}' in {} references undefined variable '{}'",
                    server.url, context, name
                )));
            }
        }
    }

    Ok(())
}

fn validate_server_url(url: &str, context: &str) -> AppResult<()> {
    if url.contains('?') || url.contains('#') {
        return Err(AppError::General(format!(
            "Server URL '{}' in {} MUST NOT include query or fragment",
            url, context
        )));
    }

    if let Ok(parsed) = Url::parse(url) {
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Err(AppError::General(format!(
                "Server URL '{}' in {} MUST NOT include query or fragment",
                url, context
            )));
        }
    }

    Ok(())
}

fn validate_server_variables(
    url: &str,
    vars: &BTreeMap<String, crate::oas::routes::shims::ShimServerVariable>,
    context: &str,
) -> AppResult<()> {
    let placeholder_re = Regex::new(r"\{([^}]+)}").expect("Invalid regex constant");
    let mut placeholder_counts: HashMap<String, usize> = HashMap::new();

    for cap in placeholder_re.captures_iter(url) {
        let name = cap[1].to_string();
        if !vars.contains_key(&name) {
            return Err(AppError::General(format!(
                "Server URL '{}' in {} references undefined variable '{}'",
                url, context, name
            )));
        }
        *placeholder_counts.entry(name).or_insert(0) += 1;
    }

    for (name, var) in vars {
        if let Some(enum_vals) = &var.enum_values {
            if enum_vals.is_empty() {
                return Err(AppError::General(format!(
                    "Server variable '{}' in {} has an empty enum",
                    name, context
                )));
            }
            if !enum_vals.contains(&var.default) {
                return Err(AppError::General(format!(
                    "Server variable '{}' in {} has default '{}' not in enum",
                    name, context, var.default
                )));
            }
        }

        let occurrences = placeholder_counts.get(name).copied().unwrap_or(0);
        if occurrences == 0 {
            return Err(AppError::General(format!(
                "Server variable '{}' in {} is not present in URL '{}'",
                name, context, url
            )));
        }
        if occurrences > 1 {
            return Err(AppError::General(format!(
                "Server variable '{}' appears more than once in URL '{}' for {}",
                name, url, context
            )));
        }
    }

    Ok(())
}

/// Validates that component keys match the required naming pattern.
pub(crate) fn validate_component_keys(components: &ShimComponents) -> AppResult<()> {
    let re = Regex::new(COMPONENT_KEY_PATTERN).expect("Invalid regex constant");

    if let Some(map) = &components.security_schemes {
        for key in map.keys() {
            validate_component_key(&re, "securitySchemes", key)?;
        }
    }

    if let Some(map) = &components.path_items {
        for key in map.keys() {
            validate_component_key(&re, "pathItems", key)?;
        }
    }

    for (section, value) in &components.extra {
        if section.starts_with("x-") {
            continue;
        }
        if !COMPONENT_SECTIONS.contains(&section.as_str()) {
            continue;
        }
        if let Some(obj) = value.as_object() {
            for key in obj.keys() {
                validate_component_key(&re, section, key)?;
            }
        }
    }

    Ok(())
}

fn validate_component_key(re: &Regex, section: &str, key: &str) -> AppResult<()> {
    if re.is_match(key) {
        Ok(())
    } else {
        Err(AppError::General(format!(
            "Component key '{}' in components.{} must match {}",
            key, section, COMPONENT_KEY_PATTERN
        )))
    }
}

/// Validates path keys and template uniqueness constraints.
pub(crate) fn validate_paths(paths: &BTreeMap<String, ShimPathItem>) -> AppResult<()> {
    let template_re = Regex::new(r"\{[^}]+}").expect("Invalid regex constant");
    let mut normalized: HashMap<String, String> = HashMap::new();

    for path in paths.keys() {
        if !path.starts_with('/') {
            return Err(AppError::General(format!(
                "Path item key '{}' must start with '/'",
                path
            )));
        }

        let normalized_path = template_re.replace_all(path, "{}").to_string();
        if let Some(existing) = normalized.get(&normalized_path) {
            if existing != path {
                return Err(AppError::General(format!(
                    "Path template '{}' conflicts with '{}' (same templated shape)",
                    path, existing
                )));
            }
        } else {
            normalized.insert(normalized_path, path.clone());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_tags_unique() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Tags, version: 1.0}
components: {}
tags:
  - name: accounts
  - name: accounts
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("Duplicate tag name"));
    }

    #[test]
    fn test_validate_tag_parent_missing() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Tags, version: 1.0}
components: {}
tags:
  - name: child
    parent: parent
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("references missing parent tag"));
    }

    #[test]
    fn test_header_object_invalid_style_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Headers, version: 1.0}
paths:
  /items:
    get:
      responses:
        '200':
          description: ok
          headers:
            X-Bad:
              style: form
              schema:
                type: string
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("style"));
    }

    #[test]
    fn test_header_object_ref_invalid_allow_empty_value() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Headers, version: 1.0}
components:
  headers:
    BadHeader:
      allowEmptyValue: true
      schema:
        type: string
paths:
  /items:
    get:
      responses:
        '200':
          description: ok
          headers:
            X-Bad:
              $ref: '#/components/headers/BadHeader'
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("allowEmptyValue"));
    }

    #[test]
    fn test_content_type_header_ignored_in_validation() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Headers, version: 1.0}
paths:
  /items:
    get:
      responses:
        '200':
          description: ok
          headers:
            Content-Type: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        assert!(validate_openapi_root(&openapi).is_ok());
    }

    #[test]
    fn test_response_media_type_example_and_examples_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Media, version: 1.0}
paths:
  /items:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              example: {id: 1}
              examples:
                sample:
                  value: {id: 1}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("example and examples"));
    }

    #[test]
    fn test_request_body_media_type_example_and_examples_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Media, version: 1.0}
paths:
  /items:
    post:
      requestBody:
        content:
          application/json:
            example: {id: 1}
            examples:
              sample:
                value: {id: 1}
      responses:
        '200':
          description: ok
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("example and examples"));
    }

    #[test]
    fn test_validate_tag_parent_cycle() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Tags, version: 1.0}
components: {}
tags:
  - name: a
    parent: b
  - name: b
    parent: a
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("Tag hierarchy contains a cycle"));
    }

    #[test]
    fn test_validate_server_url_rejects_query() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Servers, version: 1.0}
components: {}
servers:
  - url: https://example.com/api?debug=true
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("MUST NOT include query or fragment"));
    }

    #[test]
    fn test_validate_server_variable_enum_empty() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Servers, version: 1.0}
components: {}
servers:
  - url: https://{env}.example.com
    variables:
      env:
        enum: []
        default: dev
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("has an empty enum"));
    }

    #[test]
    fn test_validate_server_variable_enum_default() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Servers, version: 1.0}
components: {}
servers:
  - url: https://{env}.example.com
    variables:
      env:
        enum: [prod]
        default: dev
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("default 'dev' not in enum"));
    }

    #[test]
    fn test_validate_server_variable_duplicate_placeholder() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Servers, version: 1.0}
components: {}
servers:
  - url: https://{env}.{env}.example.com
    variables:
      env:
        default: prod
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("appears more than once"));
    }

    #[test]
    fn test_validate_server_variable_missing_definition() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Servers, version: 1.0}
components: {}
servers:
  - url: https://{env}.example.com
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("references undefined variable"));
    }

    #[test]
    fn test_validate_server_variable_unused() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Servers, version: 1.0}
components: {}
servers:
  - url: https://example.com
    variables:
      env:
        default: prod
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("is not present in URL"));
    }

    #[test]
    fn test_validate_server_name_unique() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Servers, version: 1.0}
components: {}
servers:
  - url: https://example.com/v1
    name: prod
  - url: https://example.com/v2
    name: prod
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("Duplicate server name"));
    }

    #[test]
    fn test_callback_external_docs_validation() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Callbacks, version: 1.0}
paths:
  /subscribe:
    post:
      callbacks:
        onEvent:
          '{$request.body#/callbackUrl}':
            post:
              externalDocs:
                url: not a url
              responses:
                '200':
                  description: ok
      responses:
        '200':
          description: ok
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("externalDocs.url"));
    }

    #[test]
    fn test_validate_info_terms_of_service_uri_invalid() {
        let yaml = r#"
openapi: 3.2.0
info:
  title: Info
  version: 1.0
  termsOfService: "not a uri"
components: {}
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("info.termsOfService"));
    }

    #[test]
    fn test_validate_contact_email_invalid() {
        let yaml = r#"
openapi: 3.2.0
info:
  title: Info
  version: 1.0
  contact:
    email: not-an-email
components: {}
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("info.contact.email"));
    }

    #[test]
    fn test_validate_license_identifier_and_url_mutually_exclusive() {
        let yaml = r#"
openapi: 3.2.0
info:
  title: Info
  version: 1.0
  license:
    name: MIT
    identifier: MIT
    url: https://example.com/license
components: {}
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("info.license"));
    }

    #[test]
    fn test_validate_security_scheme_oauth2_missing_flows() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Sec, version: 1.0}
components:
  securitySchemes:
    oauth2:
      type: oauth2
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("must define 'flows'"));
    }

    #[test]
    fn test_validate_security_scheme_oauth2_flow_missing_urls() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Sec, version: 1.0}
components:
  securitySchemes:
    oauth2:
      type: oauth2
      flows:
        authorizationCode:
          authorizationUrl: https://auth.example.com/authorize
          scopes: {}
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("missing tokenUrl"));
    }

    #[test]
    fn test_validate_response_code_key_invalid() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Resp, version: 1.0}
paths:
  /ping:
    get:
      responses:
        '2AB':
          description: nope
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("must be an HTTP status code or range"));
    }

    #[test]
    fn test_validate_responses_requires_response() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Resp, version: 1.0}
paths:
  /ping:
    get:
      responses: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("must define at least one response"));
    }

    #[test]
    fn test_validate_responses_default_only_allowed() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Resp, version: 1.0}
paths:
  /ping:
    get:
      responses:
        default:
          description: fallback
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        assert!(validate_openapi_root(&openapi).is_ok());
    }

    #[test]
    fn test_validate_response_description_required() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Resp, version: 1.0}
paths:
  /ping:
    get:
      responses:
        '200': { description: "" }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("Response description"));
    }

    #[test]
    fn test_validate_response_ref_description_override() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Resp, version: 1.0}
paths:
  /ping:
    get:
      responses:
        '200':
          $ref: '#/components/responses/Blank'
          description: OK
components:
  responses:
    Blank:
      description: ""
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        validate_openapi_root(&openapi).unwrap();
    }

    #[test]
    fn test_validate_request_body_requires_content() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Req, version: 1.0}
paths:
  /ping:
    post:
      requestBody:
        content: {}
      responses:
        '200': { description: OK }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("requestBody"));
        assert!(format!("{err}").contains("content"));
    }

    #[test]
    fn test_validate_request_body_component_requires_content() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Req, version: 1.0}
paths:
  /ping:
    post:
      requestBody:
        $ref: '#/components/requestBodies/EmptyBody'
      responses:
        '200': { description: OK }
components:
  requestBodies:
    EmptyBody:
      content: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("requestBody"));
        assert!(msg.contains("content"));
    }

    #[test]
    fn test_validate_path_item_ref_with_siblings_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Paths, version: 1.0}
paths:
  /pets:
    $ref: '#/components/pathItems/Pets'
    get:
      responses:
        '200': { description: OK }
components:
  pathItems:
    Pets:
      get:
        responses:
          '200': { description: OK }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("defines $ref alongside other Path Item fields"));
    }

    #[test]
    fn test_validate_security_requirement_unknown_scheme() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Sec, version: 1.0}
components:
  securitySchemes:
    ApiKey:
      type: apiKey
      name: api-key
      in: header
security:
  - Missing: []
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("does not match a known security scheme"));
    }

    #[test]
    fn test_validate_security_requirement_uri_allowed() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Sec, version: 1.0}
components: {}
security:
  - https://example.com/schemes/Auth: []
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        validate_openapi_root(&openapi).unwrap();
    }

    #[test]
    fn test_validate_security_definitions_basic_swagger2() {
        let yaml = r#"
swagger: "2.0"
info: {title: Sec, version: 1.0}
securityDefinitions:
  basicAuth:
    type: basic
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        validate_openapi_root(&openapi).unwrap();
    }

    #[test]
    fn test_validate_response_link_missing_operation() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Links, version: 1.0}
paths:
  /items:
    get:
      responses:
        '200':
          description: ok
          links:
            MissingOp:
              parameters:
                id: $response.body#/id
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("must define exactly one of"));
    }

    #[test]
    fn test_validate_response_link_both_operations() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Links, version: 1.0}
paths:
  /items:
    get:
      responses:
        '200':
          description: ok
          links:
            BadLink:
              operationId: getItem
              operationRef: '#/paths/~1items/get'
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("must define exactly one of"));
    }

    #[test]
    fn test_validate_response_link_ref_cycle() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Links, version: 1.0}
components:
  links:
    SelfLink:
      $ref: '#/components/links/SelfLink'
paths:
  /items:
    get:
      responses:
        '200':
          description: ok
          links:
            Self:
              $ref: '#/components/links/SelfLink'
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("Link reference cycle"));
    }

    #[test]
    fn test_validate_response_link_server_invalid_url() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Links, version: 1.0}
paths:
  /items:
    get:
      responses:
        '200':
          description: ok
          links:
            WithServer:
              operationId: getItem
              server:
                url: https://example.com/api?debug=true
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("MUST NOT include query or fragment"));
    }

    #[test]
    fn test_validate_response_link_name_pattern() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Links, version: 1.0}
paths:
  /items:
    get:
      responses:
        '200':
          description: ok
          links:
            bad link:
              operationId: getItem
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("Component key"));
    }
}
