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
use crate::oas::models::RuntimeExpression;
use crate::oas::ref_utils::extract_component_name;
use crate::oas::resolver::ShimParameter;
use crate::oas::routes::shims::ShimRequestBody;
use crate::oas::routes::shims::{
    ShimComponents, ShimExternalDocs, ShimOpenApi, ShimPathItem, ShimSecurityScheme, ShimServer,
};
use regex::Regex;
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, HashMap, HashSet};
use url::Url;
use utoipa::openapi::RefOr;

const COMPONENT_KEY_PATTERN: &str = r"^[a-zA-Z0-9._-]+$";
const RESERVED_PATH_METHODS: [&str; 9] = [
    "get", "post", "put", "delete", "patch", "options", "head", "trace", "query",
];
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
        && openapi.paths.is_none()
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
    if let Some(paths) = &openapi.paths {
        validate_paths(&paths.items, openapi.components.as_ref())?;
    }
    validate_responses(openapi)?;
    validate_request_bodies(openapi)?;
    validate_headers(openapi)?;
    validate_parameters(openapi)?;
    validate_component_examples(openapi)?;
    validate_component_media_types(openapi)?;
    validate_schema_discriminators(openapi)?;
    validate_schema_metadata(openapi)?;

    Ok(())
}

fn validate_parameters(openapi: &ShimOpenApi) -> AppResult<()> {
    let components = openapi.components.as_ref();
    let is_oas3 = openapi.openapi.is_some();
    if let Some(paths) = &openapi.paths {
        for (path, path_item) in &paths.items {
            validate_path_item_parameters(
                path_item,
                &format!("paths.{}", path),
                components,
                is_oas3,
            )?;
        }
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in &webhooks.items {
            if let utoipa::openapi::RefOr::T(path_item) = item_or_ref {
                validate_path_item_parameters(
                    path_item,
                    &format!("webhooks.{}", name),
                    components,
                    is_oas3,
                )?;
            }
        }
    }

    if let Some(components) = components {
        if let Some(values) = components.extra.get("parameters") {
            if let Some(map) = values.as_object() {
                for (name, param_val) in map {
                    if let Ok(param) = serde_json::from_value::<ShimParameter>(param_val.clone()) {
                        validate_parameter_content(
                            &param,
                            &format!("components.parameters.{}", name),
                            Some(components),
                        )?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn validate_path_item_parameters(
    path_item: &ShimPathItem,
    context: &str,
    components: Option<&ShimComponents>,
    is_oas3: bool,
) -> AppResult<()> {
    validate_additional_operations(path_item, context)?;
    let common_params =
        resolve_parameters_for_validation(path_item.parameters.as_deref(), components);
    validate_parameter_list(
        path_item.parameters.as_deref(),
        &format!("{}.parameters", context),
        components,
        is_oas3,
    )?;

    let validate_op = |label: &str, op: &Option<crate::oas::routes::shims::ShimOperation>| {
        if let Some(op) = op {
            validate_parameter_list(
                op.parameters.as_deref(),
                &format!("{}.{}.parameters", context, label),
                components,
                is_oas3,
            )?;
            let mut combined = common_params.clone();
            combined.extend(resolve_parameters_for_validation(
                op.parameters.as_deref(),
                components,
            ));
            validate_querystring_rules(&combined, &format!("{}.{}", context, label))?;
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
            validate_parameter_list(
                op.parameters.as_deref(),
                &format!("{}.{}.parameters", context, method),
                components,
                is_oas3,
            )?;
            let mut combined = common_params.clone();
            combined.extend(resolve_parameters_for_validation(
                op.parameters.as_deref(),
                components,
            ));
            validate_querystring_rules(&combined, &format!("{}.{}", context, method))?;
        }
    }

    Ok(())
}

fn validate_additional_operations(path_item: &ShimPathItem, context: &str) -> AppResult<()> {
    let Some(additional) = &path_item.additional_operations else {
        return Ok(());
    };

    for method in additional.keys() {
        if RESERVED_PATH_METHODS
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(method))
        {
            return Err(AppError::General(format!(
                "{} additionalOperations must not define reserved method '{}'",
                context, method
            )));
        }
    }

    Ok(())
}

/// Resolves parameters for validation, following `$ref` when possible.
fn resolve_parameters_for_validation(
    params: Option<&[RefOr<ShimParameter>]>,
    components: Option<&ShimComponents>,
) -> Vec<ShimParameter> {
    let Some(params) = params else {
        return Vec::new();
    };

    params
        .iter()
        .filter_map(|param| match param {
            RefOr::T(p) => Some(p.clone()),
            RefOr::Ref(r) => resolve_parameter_ref_for_validation(r, components),
        })
        .collect()
}

fn validate_parameter_list(
    params: Option<&[RefOr<ShimParameter>]>,
    context: &str,
    components: Option<&ShimComponents>,
    is_oas3: bool,
) -> AppResult<()> {
    let Some(params) = params else {
        return Ok(());
    };

    let resolved = resolve_parameters_for_validation(Some(params), components);
    validate_parameter_duplicates(&resolved, context)?;

    for (idx, param) in params.iter().enumerate() {
        let ctx = format!("{}[{}]", context, idx);
        let resolved = match param {
            RefOr::T(p) => Some(p.clone()),
            RefOr::Ref(r) => resolve_parameter_ref_for_validation(r, components),
        };
        if let Some(p) = resolved {
            validate_parameter_rules(&p, &ctx, is_oas3)?;
            validate_parameter_content(&p, &ctx, components)?;
        }
    }

    Ok(())
}

/// Validates that a parameter list does not contain duplicate name+location pairs.
fn validate_parameter_duplicates(params: &[ShimParameter], context: &str) -> AppResult<()> {
    let mut seen = HashSet::new();
    for param in params {
        let key = (param.name.clone(), param.parameter_in.clone());
        if !seen.insert(key.clone()) {
            return Err(AppError::General(format!(
                "{} defines duplicate parameter '{}' in '{}'",
                context, key.0, key.1
            )));
        }
    }
    Ok(())
}

/// Validates Parameter Object rules that are independent of serialization details.
fn validate_parameter_rules(param: &ShimParameter, context: &str, is_oas3: bool) -> AppResult<()> {
    let location = param.parameter_in.as_str();

    if location == "path" && !param.required {
        return Err(AppError::General(format!(
            "{} path parameter must set required: true",
            context
        )));
    }

    if param.allow_empty_value.unwrap_or(false) && location != "query" {
        return Err(AppError::General(format!(
            "{} uses allowEmptyValue but is not in 'query'",
            context
        )));
    }

    if is_oas3 {
        let has_schema = param.schema.is_some() || param.raw.get("schema").is_some();
        let has_content = param.content.as_ref().is_some() || param.raw.get("content").is_some();

        if has_schema && has_content {
            return Err(AppError::General(format!(
                "{} must not define both 'schema' and 'content'",
                context
            )));
        }

        if !has_schema && !has_content {
            return Err(AppError::General(format!(
                "{} must define either 'schema' or 'content'",
                context
            )));
        }

        if location == "querystring" {
            if has_schema {
                return Err(AppError::General(format!(
                    "{} querystring parameter must use 'content' instead of 'schema'",
                    context
                )));
            }
            if !has_content {
                return Err(AppError::General(format!(
                    "{} querystring parameter must define 'content'",
                    context
                )));
            }
        }
    }

    Ok(())
}

fn validate_parameter_content(
    param: &ShimParameter,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    let Some(content) = param.raw.get("content") else {
        return Ok(());
    };
    let Some(map) = content.as_object() else {
        return Ok(());
    };

    if map.len() != 1 {
        return Err(AppError::General(format!(
            "{} must define exactly one media type in content",
            context
        )));
    }

    validate_media_type_examples(content, &format!("{}.content", context))?;
    validate_media_type_encodings(map, &format!("{}.content", context), components)?;

    Ok(())
}

/// Validates querystring parameter constraints for a combined parameter list.
fn validate_querystring_rules(params: &[ShimParameter], context: &str) -> AppResult<()> {
    let mut querystring_count = 0;
    let mut query_count = 0;

    for param in params {
        match param.parameter_in.as_str() {
            "querystring" => querystring_count += 1,
            "query" => query_count += 1,
            _ => {}
        }
    }

    if querystring_count > 1 {
        return Err(AppError::General(format!(
            "{} defines multiple querystring parameters",
            context
        )));
    }

    if querystring_count > 0 && query_count > 0 {
        return Err(AppError::General(format!(
            "{} mixes 'querystring' and 'query' parameters",
            context
        )));
    }

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

    if let Some(paths) = &openapi.paths {
        for (path, path_item) in &paths.items {
            validate_path_item_servers(path_item, &format!("paths.{}", path))?;
        }
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in &webhooks.items {
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

    if let Some(paths) = &openapi.paths {
        for (path, path_item) in &paths.items {
            validate_path_item_external_docs(path_item, &format!("paths.{}", path), components)?;
        }
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in &webhooks.items {
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
        let (cb_map, _, _) =
            crate::oas::routes::callbacks::resolve_callback_object(cb_ref, components, None, None)?;
        for (expr, path_item) in cb_map {
            let cb_ctx = format!("{}.{}.{}", context, name, expr);
            RuntimeExpression::parse_expression(&expr).map_err(|e| {
                AppError::General(format!("Invalid callback expression in {}: {}", cb_ctx, e))
            })?;
            validate_path_item_external_docs(&path_item, &cb_ctx, components)?;
            validate_additional_operations(&path_item, &cb_ctx)?;
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

    if let Some(paths) = &openapi.paths {
        for (path, path_item) in &paths.items {
            validate_path_item_security(path_item, &format!("paths.{}", path), &known)?;
        }
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in &webhooks.items {
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
    if let Some(paths) = &openapi.paths {
        for (path, path_item) in &paths.items {
            validate_path_item_responses(path_item, &format!("paths.{}", path), components)?;
        }
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in &webhooks.items {
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
    if let Some(paths) = &openapi.paths {
        for (path, path_item) in &paths.items {
            validate_path_item_request_bodies(path_item, &format!("paths.{}", path), components)?;
        }
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in &webhooks.items {
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
        utoipa::openapi::RefOr::T(b) => validate_request_body_content(b, context, components),
        utoipa::openapi::RefOr::Ref(r) => {
            if let Some(resolved) = resolve_request_body_from_components(
                &r.ref_location,
                components,
                &mut HashSet::new(),
            ) {
                validate_request_body_content(&resolved, context, components)?;
            }
            Ok(())
        }
    }
}

fn validate_request_body_content(
    body: &ShimRequestBody,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    if body.inner.content.is_empty() {
        return Err(AppError::General(format!(
            "{} must define at least one media type in content",
            context
        )));
    }
    if let Some(content) = body.raw.get("content") {
        validate_media_type_examples(content, &format!("{}.content", context))?;
        if let Some(map) = content.as_object() {
            validate_media_type_encodings(map, &format!("{}.content", context), components)?;
        }
    }
    Ok(())
}

fn validate_media_type_encodings(
    content: &serde_json::Map<String, JsonValue>,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    for (media_type, media_val) in content {
        let Some(obj) = media_val.as_object() else {
            continue;
        };
        let has_encoding = obj.contains_key("encoding");
        let has_prefix = obj.contains_key("prefixEncoding");
        let has_item = obj.contains_key("itemEncoding");
        let has_item_schema = obj.contains_key("itemSchema");

        if has_encoding && (has_prefix || has_item) {
            return Err(AppError::General(format!(
                "{}.{} cannot define both 'encoding' and positional encoding fields",
                context, media_type
            )));
        }

        let normalized = normalize_media_type(media_type);
        let is_form = normalized == "application/x-www-form-urlencoded";
        let is_multipart = normalized.starts_with("multipart/");

        if has_encoding && !(is_form || is_multipart) {
            return Err(AppError::General(format!(
                "{}.{} uses 'encoding' but media type '{}' is not form or multipart",
                context, media_type, media_type
            )));
        }

        if (has_prefix || has_item) && !is_multipart {
            return Err(AppError::General(format!(
                "{}.{} uses positional encoding but media type '{}' is not multipart",
                context, media_type, media_type
            )));
        }

        if has_item_schema && !is_sequential_media_type(&normalized) {
            return Err(AppError::General(format!(
                "{}.{} defines 'itemSchema' but media type '{}' is not sequential",
                context, media_type, media_type
            )));
        }

        if let Some(encoding) = obj.get("encoding").and_then(|v| v.as_object()) {
            for (prop, enc_val) in encoding {
                let enc_ctx = format!("{}.{}.encoding.{}", context, media_type, prop);
                validate_encoding_headers(enc_val, &enc_ctx, components)?;
            }
        }
        if let Some(prefix) = obj.get("prefixEncoding").and_then(|v| v.as_array()) {
            for (idx, enc_val) in prefix.iter().enumerate() {
                let enc_ctx = format!("{}.{}.prefixEncoding[{}]", context, media_type, idx);
                validate_encoding_headers(enc_val, &enc_ctx, components)?;
            }
        }
        if let Some(item_enc) = obj.get("itemEncoding") {
            let enc_ctx = format!("{}.{}.itemEncoding", context, media_type);
            validate_encoding_headers(item_enc, &enc_ctx, components)?;
        }
    }

    Ok(())
}

fn validate_encoding_headers(
    encoding_val: &serde_json::Value,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    let Some(obj) = encoding_val.as_object() else {
        return Ok(());
    };
    let Some(headers) = obj.get("headers").and_then(|h| h.as_object()) else {
        // Continue to nested encoding checks even if headers are absent.
        return validate_nested_encoding(obj, context, components);
    };

    for (name, header_val) in headers {
        if name.eq_ignore_ascii_case("content-type") {
            continue;
        }
        let mut visited = HashSet::new();
        validate_header_value(
            header_val,
            &format!("{}.headers.{}", context, name),
            components,
            &mut visited,
        )?;
    }

    validate_nested_encoding(obj, context, components)
}

fn validate_nested_encoding(
    obj: &serde_json::Map<String, serde_json::Value>,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    if let Some(nested) = obj.get("encoding").and_then(|v| v.as_object()) {
        for (prop, enc_val) in nested {
            let enc_ctx = format!("{}.encoding.{}", context, prop);
            validate_encoding_headers(enc_val, &enc_ctx, components)?;
        }
    }

    if let Some(prefix) = obj.get("prefixEncoding").and_then(|v| v.as_array()) {
        for (idx, enc_val) in prefix.iter().enumerate() {
            let enc_ctx = format!("{}.prefixEncoding[{}]", context, idx);
            validate_encoding_headers(enc_val, &enc_ctx, components)?;
        }
    }

    if let Some(item) = obj.get("itemEncoding") {
        let enc_ctx = format!("{}.itemEncoding", context);
        validate_encoding_headers(item, &enc_ctx, components)?;
    }

    Ok(())
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
    matches!(
        media_type,
        "application/jsonl"
            | "application/x-ndjson"
            | "application/json-seq"
            | "application/geo+json-seq"
            | "text/event-stream"
    ) || media_type.starts_with("multipart/")
        || media_type.ends_with("+jsonl")
        || media_type.ends_with("+ndjson")
        || media_type.ends_with("+json-seq")
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

    if let Some(paths) = &openapi.paths {
        for (path, path_item) in &paths.items {
            validate_path_item_headers(path_item, &format!("paths.{}", path), components)?;
        }
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in &webhooks.items {
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
        if let Some(examples_val) = obj.get("examples") {
            let Some(examples) = examples_val.as_object() else {
                return Err(AppError::General(format!(
                    "{}.{} examples must be an object",
                    context, media_type
                )));
            };
            for (name, example_val) in examples {
                validate_example_object_value(
                    example_val,
                    &format!("{}.{}.examples.{}", context, media_type, name),
                )?;
            }
        }
    }
    Ok(())
}

/// Validates Example Objects under `components.examples`.
fn validate_component_examples(openapi: &ShimOpenApi) -> AppResult<()> {
    if openapi.openapi.is_none() {
        return Ok(());
    }

    let Some(components) = openapi.components.as_ref() else {
        return Ok(());
    };
    let Some(examples_val) = components.extra.get("examples") else {
        return Ok(());
    };
    let Some(examples) = examples_val.as_object() else {
        return Err(AppError::General(
            "components.examples must be an object".to_string(),
        ));
    };
    for (name, example_val) in examples {
        validate_example_object_value(example_val, &format!("components.examples.{}", name))?;
    }

    Ok(())
}

fn validate_component_media_types(openapi: &ShimOpenApi) -> AppResult<()> {
    if openapi.openapi.is_none() {
        return Ok(());
    }

    let Some(components) = openapi.components.as_ref() else {
        return Ok(());
    };
    let Some(media_types_val) = components.extra.get("mediaTypes") else {
        return Ok(());
    };
    let Some(media_types) = media_types_val.as_object() else {
        return Ok(());
    };

    for (name, media_val) in media_types {
        validate_media_type_object(media_val, &format!("components.mediaTypes.{}", name))?;
    }

    Ok(())
}

fn validate_media_type_object(value: &serde_json::Value, context: &str) -> AppResult<()> {
    let Some(obj) = value.as_object() else {
        return Err(AppError::General(format!(
            "Media type object in {} must be an object",
            context
        )));
    };

    if obj.contains_key("example") && obj.contains_key("examples") {
        return Err(AppError::General(format!(
            "{} must not define both example and examples",
            context
        )));
    }

    if let Some(examples_val) = obj.get("examples") {
        let Some(examples) = examples_val.as_object() else {
            return Err(AppError::General(format!(
                "{} examples must be an object",
                context
            )));
        };
        for (name, example_val) in examples {
            validate_example_object_value(example_val, &format!("{}.examples.{}", context, name))?;
        }
    }

    let has_encoding = obj.contains_key("encoding");
    let has_prefix = obj.contains_key("prefixEncoding");
    let has_item = obj.contains_key("itemEncoding");
    if has_encoding && (has_prefix || has_item) {
        return Err(AppError::General(format!(
            "{} must not define both 'encoding' and positional encoding fields",
            context
        )));
    }

    Ok(())
}

/// Validates field exclusivity rules for an Example Object.
pub(crate) fn validate_example_object_value(
    value: &serde_json::Value,
    context: &str,
) -> AppResult<()> {
    let Some(obj) = value.as_object() else {
        return Err(AppError::General(format!(
            "Example object in {} must be an object",
            context
        )));
    };

    if obj.contains_key("$ref") {
        return Ok(());
    }

    let has_value = obj.contains_key("value");
    let has_data = obj.contains_key("dataValue");
    let has_serialized = obj.contains_key("serializedValue");
    let has_external = obj.contains_key("externalValue");

    if has_value && (has_data || has_serialized || has_external) {
        return Err(AppError::General(format!(
            "Example object in {} must not define 'value' with dataValue/serializedValue/externalValue",
            context
        )));
    }

    if has_serialized && has_external {
        return Err(AppError::General(format!(
            "Example object in {} must not define both serializedValue and externalValue",
            context
        )));
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
    if let Some(paths) = &openapi.paths {
        for (path, path_item) in &paths.items {
            validate_path_item_ref(path_item, &format!("paths.{}", path))?;
            validate_additional_operations(path_item, &format!("paths.{}", path))?;
        }
    }

    if let Some(webhooks) = &openapi.webhooks {
        for (name, item_or_ref) in &webhooks.items {
            if let utoipa::openapi::RefOr::T(path_item) = item_or_ref {
                validate_path_item_ref(path_item, &format!("webhooks.{}", name))?;
                validate_additional_operations(path_item, &format!("webhooks.{}", name))?;
            }
        }
    }

    if let Some(components) = &openapi.components {
        if let Some(path_items) = &components.path_items {
            for (name, ref_or) in path_items {
                if let utoipa::openapi::RefOr::T(path_item) = ref_or {
                    validate_path_item_ref(path_item, &format!("components.pathItems.{}", name))?;
                    validate_additional_operations(
                        path_item,
                        &format!("components.pathItems.{}", name),
                    )?;
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
            .map_or(false, |p: &Vec<RefOr<ShimParameter>>| !p.is_empty())
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

    if let Some(op_ref) = obj
        .get("operationRef")
        .or_else(|| obj.get("operation_ref"))
        .and_then(|v| v.as_str())
    {
        validate_uri_reference(op_ref, &format!("{}.operationRef", context))?;
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

    let normalized = normalize_server_url_for_parse(url);
    if normalized.chars().any(|c| c.is_whitespace()) {
        return Err(AppError::General(format!(
            "Server URL '{}' in {} is not a valid URI reference",
            url, context
        )));
    }
    if let Ok(parsed) = Url::parse(&normalized) {
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Err(AppError::General(format!(
                "Server URL '{}' in {} MUST NOT include query or fragment",
                url, context
            )));
        }
        return Ok(());
    }

    // Validate relative references by joining against a dummy base.
    let base = Url::parse("https://example.com").expect("valid base url");
    if base.join(&normalized).is_err() {
        return Err(AppError::General(format!(
            "Server URL '{}' in {} is not a valid URI reference",
            url, context
        )));
    }

    Ok(())
}

fn normalize_server_url_for_parse(url: &str) -> String {
    let placeholder_re = Regex::new(r"\{[^}]+}").expect("Invalid regex constant");
    placeholder_re.replace_all(url, "placeholder").to_string()
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

/// Validates path keys, template uniqueness, and template parameter coverage.
pub(crate) fn validate_paths(
    paths: &BTreeMap<String, ShimPathItem>,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    let template_re = Regex::new(r"\{[^}]+}").expect("Invalid regex constant");
    let mut normalized: HashMap<String, String> = HashMap::new();

    for (path, path_item) in paths {
        if !path.starts_with('/') {
            return Err(AppError::General(format!(
                "Path item key '{}' must start with '/'",
                path
            )));
        }
        if path.contains('?') || path.contains('#') {
            return Err(AppError::General(format!(
                "Path item key '{}' must not include query or fragment",
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

        let templates = extract_path_templates(path);
        if templates.is_empty() {
            continue;
        }
        if let Some(dupe) = first_duplicate_template(&templates) {
            return Err(AppError::General(format!(
                "Path template '{}' contains duplicate parameter '{{{}}}'",
                path, dupe
            )));
        }

        let mut resolved = path_item.clone();
        if let Some(ref_path) = path_item.ref_path.as_deref() {
            resolved =
                crate::oas::routes::resolve_path_item_ref(ref_path, components, paths, None, None)?;
        }
        if !path_item_has_operations(&resolved) {
            continue;
        }
        validate_path_template_params(path, &templates, &resolved, components)?;
    }

    Ok(())
}

fn extract_path_templates(path: &str) -> Vec<String> {
    let template_re = Regex::new(r"\{([^}]+)}").expect("Invalid regex constant");
    template_re
        .captures_iter(path)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

fn first_duplicate_template(templates: &[String]) -> Option<String> {
    let mut seen = HashSet::new();
    for name in templates {
        if !seen.insert(name) {
            return Some(name.clone());
        }
    }
    None
}

fn path_item_has_operations(item: &ShimPathItem) -> bool {
    item.get.is_some()
        || item.post.is_some()
        || item.put.is_some()
        || item.delete.is_some()
        || item.patch.is_some()
        || item.options.is_some()
        || item.head.is_some()
        || item.trace.is_some()
        || item.query.is_some()
        || item
            .additional_operations
            .as_ref()
            .is_some_and(|ops| !ops.is_empty())
}

fn validate_path_template_params(
    path: &str,
    templates: &[String],
    path_item: &ShimPathItem,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    let common = collect_path_param_names(path_item.parameters.as_deref(), components);

    let check_op = |label: &str, op: &Option<crate::oas::routes::shims::ShimOperation>| {
        let Some(op) = op else {
            return Ok(());
        };
        let mut available = common.clone();
        available.extend(collect_path_param_names(
            op.parameters.as_deref(),
            components,
        ));
        for template in templates {
            if !available.contains(template) {
                return Err(AppError::General(format!(
                    "Path '{}' operation '{}' is missing path parameter '{{{}}}'",
                    path, label, template
                )));
            }
        }
        Ok(())
    };

    check_op("get", &path_item.get)?;
    check_op("post", &path_item.post)?;
    check_op("put", &path_item.put)?;
    check_op("delete", &path_item.delete)?;
    check_op("patch", &path_item.patch)?;
    check_op("options", &path_item.options)?;
    check_op("head", &path_item.head)?;
    check_op("trace", &path_item.trace)?;
    check_op("query", &path_item.query)?;
    if let Some(additional) = &path_item.additional_operations {
        for (method, op) in additional {
            check_op(method, &Some(op.clone()))?;
        }
    }

    Ok(())
}

fn collect_path_param_names(
    params: Option<&[RefOr<ShimParameter>]>,
    components: Option<&ShimComponents>,
) -> HashSet<String> {
    let mut names = HashSet::new();
    let Some(params) = params else {
        return names;
    };
    for param in params {
        let resolved = match param {
            RefOr::T(p) => Some(p.clone()),
            RefOr::Ref(r) => resolve_parameter_ref_for_validation(r, components),
        };
        if let Some(p) = resolved {
            if p.parameter_in == "path" {
                names.insert(p.name.clone());
            }
        }
    }
    names
}

fn resolve_parameter_ref_for_validation(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
) -> Option<ShimParameter> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let ref_name = extract_component_name(&r.ref_location, self_uri, "parameters")?;
    comps
        .extra
        .get("parameters")
        .and_then(|p| p.get(&ref_name))
        .and_then(|param_json| serde_json::from_value::<ShimParameter>(param_json.clone()).ok())
}

fn validate_schema_discriminators(openapi: &ShimOpenApi) -> AppResult<()> {
    if openapi.openapi.is_none() {
        return Ok(());
    }

    let Some(components) = openapi.components.as_ref() else {
        return Ok(());
    };
    let Some(raw_schemas) = components.extra.get("schemas") else {
        return Ok(());
    };
    let Some(schema_map) = raw_schemas.as_object() else {
        return Err(AppError::General(
            "components.schemas must be an object".to_string(),
        ));
    };

    for (name, schema_val) in schema_map {
        let ctx = format!("components.schemas.{}", name);
        validate_discriminator_in_schema(schema_val, &ctx)?;
    }

    Ok(())
}

/// Validates schema-level metadata such as `externalDocs` and `xml` objects.
fn validate_schema_metadata(openapi: &ShimOpenApi) -> AppResult<()> {
    if openapi.openapi.is_none() {
        return Ok(());
    }

    let Some(components) = openapi.components.as_ref() else {
        return Ok(());
    };
    let Some(raw_schemas) = components.extra.get("schemas") else {
        return Ok(());
    };
    let Some(schema_map) = raw_schemas.as_object() else {
        return Err(AppError::General(
            "components.schemas must be an object".to_string(),
        ));
    };

    for (name, schema_val) in schema_map {
        let ctx = format!("components.schemas.{}", name);
        validate_schema_metadata_node(schema_val, &ctx)?;
    }

    Ok(())
}

fn validate_schema_metadata_node(value: &JsonValue, context: &str) -> AppResult<()> {
    if let Some(arr) = value.as_array() {
        for (idx, item) in arr.iter().enumerate() {
            let child_ctx = format!("{}[{}]", context, idx);
            validate_schema_metadata_node(item, &child_ctx)?;
        }
        return Ok(());
    }

    let Some(obj) = value.as_object() else {
        return Ok(());
    };

    if let Some(external_docs) = obj.get("externalDocs") {
        validate_schema_external_docs(external_docs, &format!("{}.externalDocs", context))?;
    }

    if let Some(xml) = obj.get("xml") {
        validate_schema_xml_object(xml, &format!("{}.xml", context))?;
    }

    for (key, child) in obj {
        if key == "externalDocs" || key == "xml" {
            continue;
        }
        let child_ctx = format!("{}.{}", context, key);
        validate_schema_metadata_node(child, &child_ctx)?;
    }

    Ok(())
}

fn validate_schema_external_docs(value: &JsonValue, context: &str) -> AppResult<()> {
    let Some(obj) = value.as_object() else {
        return Err(AppError::General(format!("{} must be an object", context)));
    };

    let url = obj
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General(format!("{}.url must be a non-empty string", context)))?;

    validate_uri_reference(url, &format!("{}.url", context))?;

    Ok(())
}

fn validate_schema_xml_object(value: &JsonValue, context: &str) -> AppResult<()> {
    let Some(obj) = value.as_object() else {
        return Err(AppError::General(format!("{} must be an object", context)));
    };

    if let Some(node_type) = obj.get("nodeType") {
        let node = node_type
            .as_str()
            .ok_or_else(|| AppError::General(format!("{}.nodeType must be a string", context)))?;
        let valid = matches!(node, "element" | "attribute" | "text" | "cdata" | "none");
        if !valid {
            return Err(AppError::General(format!(
                "{}.nodeType must be one of element|attribute|text|cdata|none",
                context
            )));
        }

        if obj.contains_key("attribute") || obj.contains_key("wrapped") {
            return Err(AppError::General(format!(
                "{} cannot combine nodeType with attribute/wrapped",
                context
            )));
        }
    }

    if let Some(name) = obj.get("name") {
        if !name.is_string() {
            return Err(AppError::General(format!(
                "{}.name must be a string",
                context
            )));
        }
    }

    if let Some(namespace) = obj.get("namespace") {
        if !namespace.is_string() {
            return Err(AppError::General(format!(
                "{}.namespace must be a string",
                context
            )));
        }
    }

    if let Some(prefix) = obj.get("prefix") {
        if !prefix.is_string() {
            return Err(AppError::General(format!(
                "{}.prefix must be a string",
                context
            )));
        }
    }

    if let Some(attribute) = obj.get("attribute") {
        if !attribute.is_boolean() {
            return Err(AppError::General(format!(
                "{}.attribute must be a boolean",
                context
            )));
        }
    }

    if let Some(wrapped) = obj.get("wrapped") {
        if !wrapped.is_boolean() {
            return Err(AppError::General(format!(
                "{}.wrapped must be a boolean",
                context
            )));
        }
    }

    Ok(())
}
fn validate_discriminator_in_schema(value: &JsonValue, context: &str) -> AppResult<()> {
    if let Some(arr) = value.as_array() {
        for (idx, item) in arr.iter().enumerate() {
            let child_ctx = format!("{}[{}]", context, idx);
            validate_discriminator_in_schema(item, &child_ctx)?;
        }
        return Ok(());
    }

    let Some(obj) = value.as_object() else {
        return Ok(());
    };

    if let Some(discriminator) = obj.get("discriminator") {
        let disc_obj = discriminator.as_object().ok_or_else(|| {
            AppError::General(format!("Discriminator in {} must be an object", context))
        })?;

        let prop_name = disc_obj
            .get("propertyName")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppError::General(format!(
                    "Discriminator in {} must define non-empty propertyName",
                    context
                ))
            })?;

        let required = obj
            .get("required")
            .and_then(|v| v.as_array())
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let is_required = required
            .iter()
            .any(|v| v.as_str().map(|s| s == prop_name).unwrap_or(false));

        if !is_required {
            let default_mapping = disc_obj
                .get("defaultMapping")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            if default_mapping.is_none() {
                return Err(AppError::General(format!(
                    "Discriminator in {} must define defaultMapping when '{}' is optional",
                    context, prop_name
                )));
            }
        }
    }

    for (key, child) in obj {
        if key == "discriminator" {
            continue;
        }
        let child_ctx = format!("{}.{}", context, key);
        validate_discriminator_in_schema(child, &child_ctx)?;
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
    fn test_parameter_path_requires_required_true() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Params, version: 1.0}
paths:
  /items/{id}:
    get:
      parameters:
        - name: id
          in: path
          required: false
          schema: { type: string }
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("path parameter must set required"));
    }

    #[test]
    fn test_parameter_requires_schema_or_content_oas3() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Params, version: 1.0}
paths:
  /items:
    get:
      parameters:
        - name: limit
          in: query
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("must define either 'schema' or 'content'"));
    }

    #[test]
    fn test_parameter_schema_and_content_mutual_exclusive() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Params, version: 1.0}
paths:
  /items:
    get:
      parameters:
        - name: q
          in: query
          schema: { type: string }
          content:
            application/json:
              schema: { type: string }
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("must not define both 'schema' and 'content'"));
    }

    #[test]
    fn test_parameter_allow_empty_value_non_query_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Params, version: 1.0}
paths:
  /items:
    get:
      parameters:
        - name: X-Test
          in: header
          allowEmptyValue: true
          schema: { type: string }
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("allowEmptyValue"));
    }

    #[test]
    fn test_parameter_duplicate_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Params, version: 1.0}
paths:
  /items:
    get:
      parameters:
        - name: id
          in: query
          schema: { type: string }
        - name: id
          in: query
          schema: { type: string }
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("duplicate parameter"));
    }

    #[test]
    fn test_querystring_mixed_with_query_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Params, version: 1.0}
paths:
  /items:
    parameters:
      - name: qs
        in: querystring
        content:
          application/x-www-form-urlencoded:
            schema: { type: object }
    get:
      parameters:
        - name: filter
          in: query
          schema: { type: string }
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("mixes 'querystring' and 'query'"));
    }

    #[test]
    fn test_querystring_duplicate_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Params, version: 1.0}
paths:
  /items:
    get:
      parameters:
        - name: qs1
          in: querystring
          content:
            application/x-www-form-urlencoded:
              schema: { type: object }
        - name: qs2
          in: querystring
          content:
            application/x-www-form-urlencoded:
              schema: { type: object }
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("multiple querystring parameters"));
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
    fn test_parameter_content_item_schema_requires_sequential() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Params, version: 1.0}
paths:
  /items:
    get:
      parameters:
        - name: stream
          in: query
          content:
            application/json:
              itemSchema:
                type: string
      responses:
        '200':
          description: ok
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("itemSchema"));
    }

    #[test]
    fn test_media_type_example_object_conflicting_fields() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Examples, version: 1.0}
paths:
  /pets:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              examples:
                Bad:
                  value: {id: 1}
                  serializedValue: '{"id":1}'
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("examples.Bad"));
    }

    #[test]
    fn test_component_example_object_conflicting_fields() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Examples, version: 1.0}
components:
  examples:
    BadExample:
      value: {id: 1}
      externalValue: https://example.com/example.json
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("components.examples.BadExample"));
    }

    #[test]
    fn test_component_media_type_example_conflict() {
        let yaml = r#"
openapi: 3.2.0
info: {title: MediaTypes, version: 1.0}
components:
  mediaTypes:
    Json:
      example: { id: 1 }
      examples:
        one:
          value: { id: 1 }
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("components.mediaTypes.Json"));
    }

    #[test]
    fn test_component_media_type_encoding_conflict() {
        let yaml = r#"
openapi: 3.2.0
info: {title: MediaTypes, version: 1.0}
components:
  mediaTypes:
    Multipart:
      encoding: {}
      prefixEncoding: []
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("components.mediaTypes.Multipart"));
        assert!(format!("{err}").contains("encoding"));
    }

    #[test]
    fn test_request_body_encoding_requires_form_or_multipart() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Enc, version: 1.0}
paths:
  /items:
    post:
      requestBody:
        content:
          application/json:
            schema: { type: object }
            encoding:
              payload:
                contentType: text/plain
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("encoding"));
        assert!(format!("{err}").contains("media type"));
    }

    #[test]
    fn test_request_body_encoding_headers_invalid_header_object_rejected() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Enc, version: 1.0}
paths:
  /items:
    post:
      requestBody:
        content:
          multipart/form-data:
            schema:
              type: object
              properties:
                file:
                  type: string
            encoding:
              file:
                headers:
                  X-Bad:
                    name: bad
                    schema: { type: string }
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("must not define 'name' or 'in'"));
    }

    #[test]
    fn test_request_body_nested_encoding_validates() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Enc, version: 1.0}
paths:
  /items:
    post:
      requestBody:
        content:
          multipart/form-data:
            schema:
              type: object
              properties:
                payload:
                  type: object
            encoding:
              payload:
                encoding:
                  part:
                    headers:
                      X-Trace-Id:
                        schema: { type: string }
                prefixEncoding:
                  - headers:
                      X-Req:
                        schema: { type: string }
                itemEncoding:
                  headers:
                    X-Item:
                      schema: { type: string }
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        validate_openapi_root(&openapi).unwrap();
    }

    #[test]
    fn test_request_body_prefix_encoding_requires_multipart() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Enc, version: 1.0}
paths:
  /items:
    post:
      requestBody:
        content:
          application/x-www-form-urlencoded:
            schema: { type: object }
            prefixEncoding:
              - contentType: text/plain
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("positional encoding"));
    }

    #[test]
    fn test_request_body_item_schema_requires_sequential_media_type() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Enc, version: 1.0}
paths:
  /items:
    post:
      requestBody:
        content:
          application/json:
            itemSchema: { type: string }
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("itemSchema"));
        assert!(format!("{err}").contains("sequential"));
    }

    #[test]
    fn test_request_body_item_schema_allows_sequential_media_type() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Enc, version: 1.0}
paths:
  /items:
    post:
      requestBody:
        content:
          application/jsonl:
            itemSchema: { type: string }
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        assert!(validate_openapi_root(&openapi).is_ok());
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
    fn test_validate_server_url_rejects_invalid_reference() {
        let yaml = r#" 
openapi: 3.2.0
info: {title: Servers, version: 1.0} 
components: {} 
servers: 
  - url: "https://example.com/has space"
paths: {} 
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("valid URI reference"));
    }

    #[test]
    fn test_validate_server_url_allows_variables() {
        let yaml = r#" 
openapi: 3.2.0
info: {title: Servers, version: 1.0} 
components: {} 
servers: 
  - url: https://{env}.example.com/api
    variables:
      env:
        default: prod
paths: {} 
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        assert!(validate_openapi_root(&openapi).is_ok());
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
    fn test_validate_additional_operations_rejects_reserved_method() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Paths, version: 1.0}
paths:
  /pets:
    additionalOperations:
      GET:
        responses:
          '200': { description: OK }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("additionalOperations"));
        assert!(format!("{err}").contains("GET"));
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
    fn test_validate_response_link_operation_ref_uri() {
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
            BadRef: 
              operationRef: 'not a uri'
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("operationRef"));
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

    #[test]
    fn test_validate_paths_rejects_query_fragment() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Paths, version: 1.0}
paths:
  /items?debug=true:
    get:
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("must not include query or fragment"));
    }

    #[test]
    fn test_validate_paths_requires_template_params() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Paths, version: 1.0}
paths:
  /pets/{petId}:
    get:
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("missing path parameter"));
    }

    #[test]
    fn test_validate_paths_accepts_template_params_in_path_item() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Paths, version: 1.0}
paths:
  /pets/{petId}:
    parameters:
      - name: petId
        in: path
        required: true
        schema: { type: string }
    get:
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        validate_openapi_root(&openapi).unwrap();
    }

    #[test]
    fn test_validate_paths_accepts_template_params_via_component_ref() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Paths, version: 1.0}
components:
  parameters:
    PetId:
      name: petId
      in: path
      required: true
      schema: { type: string }
paths:
  /pets/{petId}:
    get:
      parameters:
        - $ref: '#/components/parameters/PetId'
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        validate_openapi_root(&openapi).unwrap();
    }

    #[test]
    fn test_validate_paths_rejects_duplicate_template_param() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Paths, version: 1.0}
paths:
  /pets/{id}/owners/{id}:
    get:
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: string }
      responses:
        '200': { description: ok }
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("duplicate parameter"));
    }

    #[test]
    fn test_validate_paths_allows_empty_path_item_with_template() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Paths, version: 1.0}
paths:
  /pets/{petId}: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        validate_openapi_root(&openapi).unwrap();
    }

    #[test]
    fn test_discriminator_requires_default_mapping_when_optional() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Disc, version: 1.0}
components:
  schemas:
    Pet:
      type: object
      discriminator:
        propertyName: petType
      oneOf:
        - $ref: '#/components/schemas/Cat'
    Cat:
      type: object
      required: [petType]
      properties:
        petType:
          const: cat
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("defaultMapping"));
    }

    #[test]
    fn test_discriminator_optional_with_default_mapping_ok() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Disc, version: 1.0}
components:
  schemas:
    Pet:
      type: object
      discriminator:
        propertyName: petType
        defaultMapping: '#/components/schemas/OtherPet'
      oneOf:
        - $ref: '#/components/schemas/Cat'
        - $ref: '#/components/schemas/OtherPet'
    Cat:
      type: object
      required: [petType]
      properties:
        petType:
          const: cat
    OtherPet:
      type: object
      properties:
        petType:
          not:
            enum: ['cat']
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        validate_openapi_root(&openapi).unwrap();
    }

    #[test]
    fn test_discriminator_required_property_ok() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Disc, version: 1.0}
components:
  schemas:
    Pet:
      type: object
      required: [petType]
      discriminator:
        propertyName: petType
      oneOf:
        - $ref: '#/components/schemas/Cat'
    Cat:
      type: object
      required: [petType]
      properties:
        petType:
          const: cat
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        validate_openapi_root(&openapi).unwrap();
    }

    #[test]
    fn test_validate_callback_expression_invalid() {
        let yaml = r#"
openapi: 3.2.0
info: {title: CB, version: 1.0}
paths:
  /subscribe:
    post:
      responses:
        '200': {description: ok}
      callbacks:
        onEvent:
          'not-a-runtime-expression':
            post:
              responses:
                '200': {description: ok}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("Invalid callback expression"));
    }

    #[test]
    fn test_schema_external_docs_url_validation() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Docs, version: 1.0}
components:
  schemas:
    Foo:
      type: object
      externalDocs:
        url: "not a uri"
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("externalDocs.url"));
    }

    #[test]
    fn test_schema_xml_node_type_validation() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Xml, version: 1.0}
components:
  schemas:
    Foo:
      type: string
      xml:
        nodeType: banana
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("nodeType"));
    }

    #[test]
    fn test_schema_xml_node_type_conflict() {
        let yaml = r#"
openapi: 3.2.0
info: {title: Xml, version: 1.0}
components:
  schemas:
    Foo:
      type: string
      xml:
        nodeType: attribute
        attribute: true
paths: {}
"#;
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).unwrap();
        let err = validate_openapi_root(&openapi).unwrap_err();
        assert!(format!("{err}").contains("nodeType"));
    }
}
