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
use crate::openapi::parse::models::RuntimeExpression;
use crate::openapi::parse::ref_utils::extract_component_name;
use crate::openapi::parse::resolver::ShimParameter;
use crate::openapi::parse::routes::shims::ShimRequestBody;
use crate::openapi::parse::routes::shims::{
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
    let has_top_level_content = [
        openapi.components.is_some(),
        openapi.paths.is_some(),
        openapi.webhooks.is_some(),
    ]
    .into_iter()
    .any(|b| b);
    if openapi.openapi.is_some() && !has_top_level_content {
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

    if let Some((components, map)) =
        components.and_then(|c| Some((c, c.extra.get("parameters")?.as_object()?)))
    {
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

    Ok(())
}

fn for_each_operation<F>(path_item: &ShimPathItem, mut f: F) -> AppResult<()>
where
    F: FnMut(&str, &crate::openapi::parse::routes::shims::ShimOperation) -> AppResult<()>,
{
    for (label, op) in [
        ("get", &path_item.get),
        ("post", &path_item.post),
        ("put", &path_item.put),
        ("delete", &path_item.delete),
        ("patch", &path_item.patch),
        ("options", &path_item.options),
        ("head", &path_item.head),
        ("trace", &path_item.trace),
        ("query", &path_item.query),
    ] {
        if let Some(op) = op {
            f(label, op)?;
        }
    }

    if let Some(additional) = &path_item.additional_operations {
        for (method, op) in additional {
            f(method, op)?;
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

    for_each_operation(path_item, |label, op| {
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
        validate_querystring_rules(&combined, &format!("{}.{}", context, label))
    })
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

        if location == "querystring" && has_schema {
            return Err(AppError::General(format!(
                "{} querystring parameter must use 'content' instead of 'schema'",
                context
            )));
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
    let map = content
        .as_object()
        .expect("ShimParameter content is object");

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
    let info = openapi
        .info
        .as_ref()
        .expect("checked in validate_openapi_root");

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

fn validate_contact(contact: &crate::openapi::parse::routes::shims::ShimContact) -> AppResult<()> {
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

fn validate_license(license: &crate::openapi::parse::routes::shims::ShimLicense) -> AppResult<()> {
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
    for_each_operation(path_item, |label, op| {
        if let Some(external) = &op.external_docs {
            let ctx = format!("{}.{}.externalDocs.url", context, label);
            validate_external_docs_reference(&ctx, external)?;
        }
        if let Some(callbacks) = &op.callbacks {
            let cb_ctx = format!("{}.{}.callbacks", context, label);
            validate_callbacks_external_docs(callbacks, &cb_ctx, components)?;
        }
        Ok(())
    })
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
        let (cb_map, _, _) = crate::openapi::parse::routes::callbacks::resolve_callback_object(
            cb_ref, components, None, None,
        )?;
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
                        if let Some((ref_name, utoipa::openapi::RefOr::T(resolved))) =
                            crate::openapi::parse::ref_utils::extract_component_name(
                                &r.ref_location,
                                openapi.self_uri.as_deref(),
                                "securitySchemes",
                            )
                            .and_then(|n| schemes.get(&n).map(|s| (n, s)))
                        {
                            validate_security_scheme(&ref_name, resolved, is_oas3)?;
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
            } else {
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
    flows: &crate::openapi::parse::routes::shims::ShimOAuthFlows,
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
    flow: &crate::openapi::parse::routes::shims::ShimOAuthFlow,
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
    oauth: &crate::openapi::parse::routes::shims::ShimOAuth2,
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
    for_each_operation(path_item, |label, op| {
        if let Some(security) = &op.security {
            let ctx = format!("{}.{}.security", context, label);
            validate_security_requirement_list(security, known_schemes, &ctx)?;
        }
        Ok(())
    })
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
    if trimmed.contains(char::is_whitespace) {
        return false;
    }
    let parts: Vec<&str> = trimmed.split('@').collect();
    [
        parts.len() == 2,
        !parts[0].is_empty(),
        parts.get(1).is_some_and(|d| d.contains('.')),
    ]
    .into_iter()
    .all(|b| b)
}

fn validate_path_item_servers(path_item: &ShimPathItem, context: &str) -> AppResult<()> {
    if let Some(servers) = &path_item.servers {
        validate_server_list(servers, &format!("{}.servers", context))?;
    }

    for_each_operation(path_item, |label, op| {
        if let Some(servers) = &op.servers {
            validate_server_list(servers, &format!("{}.{}.servers", context, label))?;
        }
        Ok(())
    })
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

    if let Some((components, map)) = openapi
        .components
        .as_ref()
        .and_then(|c| Some((c, c.extra.get("requestBodies")?.as_object()?)))
    {
        for (name, body_val) in map {
            if let Ok(body) =
                serde_json::from_value::<utoipa::openapi::RefOr<ShimRequestBody>>(body_val.clone())
            {
                let ctx = format!("components.requestBodies.{}", name);
                validate_request_body_ref_or(&body, &ctx, Some(components))?;
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
    for_each_operation(path_item, |label, op| {
        if let Some(body) = &op.request_body {
            validate_request_body_ref_or(
                body,
                &format!("{}.{}.requestBody", context, label),
                components,
            )?;
        }
        Ok(())
    })
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
    let content = body.raw.get("content").expect("non-empty content");
    validate_media_type_examples(content, &format!("{}.content", context))?;
    let map = content.as_object().expect("content is object");
    validate_media_type_encodings(map, &format!("{}.content", context), components)
}

fn validate_media_type_encodings(
    content: &serde_json::Map<String, JsonValue>,
    context: &str,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    for (media_type, media_val) in content {
        let obj = media_val.as_object().expect("media_val is object");
        let has_encoding = obj.contains_key("encoding");
        let has_positional = obj.contains_key("prefixEncoding") || obj.contains_key("itemEncoding");
        let has_item_schema = obj.contains_key("itemSchema");

        if has_encoding && has_positional {
            return Err(AppError::General(format!(
                "{}.{} cannot define both 'encoding' and positional encoding fields",
                context, media_type
            )));
        }

        let normalized = normalize_media_type(media_type);
        let is_form_or_multipart = normalized == "application/x-www-form-urlencoded"
            || normalized.starts_with("multipart/");

        if has_encoding && !is_form_or_multipart {
            return Err(AppError::General(format!(
                "{}.{} uses 'encoding' but media type '{}' is not form or multipart",
                context, media_type, media_type
            )));
        }

        if has_positional && !normalized.starts_with("multipart/") {
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
        || ["+jsonl", "+ndjson", "+json-seq"]
            .iter()
            .any(|s| media_type.ends_with(s))
}

fn validate_headers(openapi: &ShimOpenApi) -> AppResult<()> {
    // Swagger 2.0 does not define Header Objects with OAS 3.x rules.
    if openapi.openapi.is_none() {
        return Ok(());
    }

    let components = openapi.components.as_ref();

    if let Some(headers) = components
        .and_then(|c| c.extra.get("headers"))
        .and_then(|v| v.as_object())
    {
        validate_header_entries(headers, "components.headers", components)?;
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
    for_each_operation(path_item, |label, op| {
        validate_response_headers(
            &op.responses,
            &format!("{}.{}.responses", context, label),
            components,
        )
    })
}

fn validate_response_headers(
    responses: &crate::openapi::parse::routes::shims::ShimResponses,
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

        if let Some(headers) = raw_response
            .as_ref()
            .and_then(|raw| raw.get("headers"))
            .and_then(|h| h.as_object())
        {
            validate_header_entries(headers, &format!("{}.headers", resp_context), components)?;
        }
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

    if ["name", "in"].iter().any(|k| obj.contains_key(*k)) {
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
    let map = content.as_object().expect("content is object");
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

    if let Some(media_types) = openapi
        .components
        .as_ref()
        .and_then(|c| c.extra.get("mediaTypes"))
        .and_then(|v| v.as_object())
    {
        for (name, media_val) in media_types {
            validate_media_type_object(media_val, &format!("components.mediaTypes.{}", name))?;
        }
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

    if ["example", "examples"].iter().all(|k| obj.contains_key(*k)) {
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

    if [
        obj.contains_key("encoding"),
        obj.contains_key("prefixEncoding") || obj.contains_key("itemEncoding"),
    ]
    .into_iter()
    .all(|b| b)
    {
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
    let has_oas32_val = ["dataValue", "serializedValue", "externalValue"]
        .iter()
        .any(|k| obj.contains_key(*k));

    if has_value && has_oas32_val {
        return Err(AppError::General(format!(
            "Example object in {} must not define 'value' with dataValue/serializedValue/externalValue",
            context
        )));
    }

    if ["serializedValue", "externalValue"]
        .iter()
        .all(|k| obj.contains_key(*k))
    {
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
    let (comps, self_uri) = components.map(|c| (c, c.self_uri.as_deref()))?;
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
    let (comps, self_uri) = components.map(|c| (c, c.self_uri.as_deref()))?;
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

    let has_siblings = [
        path_item.summary.is_some(),
        path_item.description.is_some(),
        path_item.servers.is_some(),
        path_item.parameters.is_some(),
        path_item_has_operations(path_item),
    ]
    .into_iter()
    .any(|b| b);

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
    for_each_operation(path_item, |label, op| {
        validate_response_map(
            &op.responses,
            &format!("{}.{}.responses", context, label),
            components,
        )
    })
}

fn validate_response_map(
    responses: &crate::openapi::parse::routes::shims::ShimResponses,
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
    let raw = resolve_response_raw_from_components(ref_str, components, visited)?;
    serde_json::from_value::<utoipa::openapi::Response>(raw).ok()
}

fn resolve_response_raw_from_components(
    ref_str: &str,
    components: Option<&ShimComponents>,
    visited: &mut HashSet<String>,
) -> Option<serde_json::Value> {
    let (comps, self_uri) = components.map(|c| (c, c.self_uri.as_deref()))?;
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
    let (comps, self_uri) = components.map(|c| (c, c.self_uri.as_deref()))?;
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
    if ['?', '#'].iter().any(|c| url.contains(*c)) {
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
    if Url::parse(&normalized).is_ok() {
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
    vars: &BTreeMap<String, crate::openapi::parse::routes::shims::ShimServerVariable>,
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
        if ['?', '#'].iter().any(|c| path.contains(*c)) {
            return Err(AppError::General(format!(
                "Path item key '{}' must not include query or fragment",
                path
            )));
        }

        let normalized_path = template_re.replace_all(path, "{}").to_string();
        if let Some(existing) = normalized.insert(normalized_path, path.clone()) {
            return Err(AppError::General(format!(
                "Path template '{}' conflicts with '{}' (same templated shape)",
                path, existing
            )));
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
            resolved = crate::openapi::parse::routes::resolve_path_item_ref(
                ref_path, components, paths, None, None,
            )?;
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
    [
        item.get.is_some(),
        item.post.is_some(),
        item.put.is_some(),
        item.delete.is_some(),
        item.patch.is_some(),
        item.options.is_some(),
        item.head.is_some(),
        item.trace.is_some(),
        item.query.is_some(),
        item.additional_operations
            .as_ref()
            .is_some_and(|ops| !ops.is_empty()),
    ]
    .into_iter()
    .any(|b| b)
}

fn validate_path_template_params(
    path: &str,
    templates: &[String],
    path_item: &ShimPathItem,
    components: Option<&ShimComponents>,
) -> AppResult<()> {
    let common = collect_path_param_names(path_item.parameters.as_deref(), components);

    for_each_operation(path_item, |label, op| {
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
    })
}

fn collect_path_param_names(
    params: Option<&[RefOr<ShimParameter>]>,
    components: Option<&ShimComponents>,
) -> HashSet<String> {
    resolve_parameters_for_validation(params, components)
        .into_iter()
        .filter(|p| p.parameter_in == "path")
        .map(|p| p.name)
        .collect()
}

fn resolve_parameter_ref_for_validation(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
) -> Option<ShimParameter> {
    let (comps, self_uri) = components.map(|c| (c, c.self_uri.as_deref()))?;
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

        if ["attribute", "wrapped"]
            .iter()
            .any(|k| obj.contains_key(*k))
        {
            return Err(AppError::General(format!(
                "{} cannot combine nodeType with attribute/wrapped",
                context
            )));
        }
    }

    for field in ["name", "namespace", "prefix"] {
        if let Some(val) = obj.get(field) {
            if !val.is_string() {
                return Err(AppError::General(format!(
                    "{}.{} must be a string",
                    context, field
                )));
            }
        }
    }

    for field in ["attribute", "wrapped"] {
        if let Some(val) = obj.get(field) {
            if !val.is_boolean() {
                return Err(AppError::General(format!(
                    "{}.{} must be a boolean",
                    context, field
                )));
            }
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        validate_openapi_root(&openapi).expect("Failed to validate openapi root");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        validate_openapi_root(&openapi).expect("Failed to validate openapi root");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        validate_openapi_root(&openapi).expect("Failed to validate openapi root");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        validate_openapi_root(&openapi).expect("Failed to validate openapi root");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        validate_openapi_root(&openapi).expect("Failed to validate openapi root");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        validate_openapi_root(&openapi).expect("Failed to validate openapi root");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        validate_openapi_root(&openapi).expect("Failed to validate openapi root");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        validate_openapi_root(&openapi).expect("Failed to validate openapi root");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        validate_openapi_root(&openapi).expect("Failed to validate openapi root");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
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
        let openapi: ShimOpenApi = serde_yaml::from_str(yaml).expect("Failed to parse from string");
        let err = validate_openapi_root(&openapi).expect_err("expected error");
        assert!(format!("{err}").contains("nodeType"));

        let valid_full_yaml = r#"
openapi: 3.2.0
$self: https://example.com/api.yaml
jsonSchemaDialect: https://spec.openapis.org/oas/3.1/dialect/base
info:
  title: Full
  version: "1.0"
  termsOfService: https://example.com/tos
  contact:
    url: https://example.com/contact
    email: dev@example.com
  license:
    name: MIT
    identifier: MIT
externalDocs:
  url: https://example.com/docs
tags:
  - name: root
    externalDocs:
      url: https://example.com/tag-docs
  - name: child1
    parent: root
  - name: child2
    parent: root
servers:
  - url: /v1
security:
  - {}
  - ApiKeyH: []
  - ./sec.yaml#ext: []
  - ../sec.yaml#ext: []
  - /sec.yaml#ext: []
  - '#ext': []
webhooks:
  onHook:
    parameters:
      - name: h_q
        in: query
        allowEmptyValue: true
        schema: { type: string }
      - $ref: '#/components/parameters/MissingParam'
    servers:
      - url: https://{env}.hook.example.com
        variables:
          env:
            default: prod
            enum: [prod, staging]
    get:
      externalDocs:
        url: https://example.com/hook-docs
      servers:
        - url: https://hook2.example.com
      security:
        - ApiKeyQ: []
      requestBody:
        $ref: '#/components/requestBodies/ChainBody'
      responses:
        '2XX':
          $ref: '#/components/responses/ChainResp'
        '3XX':
          $ref: '#/components/responses/CycleResp'
        '4XX':
          $ref: '#/components/responses/MissingResp'
    post:
      parameters: []
      responses:
        '200': { description: ok }
  qsHook:
    post:
      parameters:
        - name: qs_only
          in: querystring
          content:
            application/x-www-form-urlencoded:
              example: "a=1"
              schema: { type: string }
      responses:
        '200': { description: ok }
    put:
      requestBody:
        $ref: '#/components/requestBodies/CycleBody'
      responses:
        '200':
          description: ok
          headers:
            X-Cyc:
              $ref: '#/components/headers/CycleHdr'
    additionalOperations:
      CUSTOM:
        externalDocs:
          url: https://example.com/custom-docs
        servers:
          - url: https://hook3.example.com
        security:
          - ApiKeyC: []
        parameters:
          - name: c_q
            in: query
            content:
              application/json:
                schema: { type: string }
        requestBody:
          content:
            multipart/mixed:
              itemSchema: { type: string }
              prefixEncoding:
                - contentType: text/plain
                  headers:
                    Content-Type:
                      schema: { type: string }
                    X-H:
                      $ref: '#/components/headers/ChainHdr'
              itemEncoding:
                contentType: text/plain
                encoding:
                  sub:
                    contentType: text/plain
                prefixEncoding:
                  - contentType: text/plain
                itemEncoding:
                  contentType: text/plain
        responses:
          '200':
            description: ok
            headers:
              Content-Type:
                schema: { type: string }
              X-Missing-Ref:
                $ref: '#/components/headers/DoesNotExist'
            links:
              L1:
                $ref: '#/components/links/ValidLink'
              L2:
                operationId: getSomething
        callbacks:
          cb1:
            '{$request.query.url}':
              get:
                responses:
                  '200': { description: ok }
  refHook:
    $ref: '#/components/pathItems/SharedPi'
paths:
  /ref-path/{id}:
    $ref: '#/components/pathItems/SharedPi'
components:
  x-ext: 123
  unknownSection: {}
  pathItems:
    SharedPi:
      parameters:
        - name: id
          in: path
          required: true
          schema: { type: string }
      put:
        responses: { '200': { description: ok } }
      delete:
        responses: { '200': { description: ok } }
      patch:
        responses: { '200': { description: ok } }
      options:
        responses: { '200': { description: ok } }
      head:
        responses: { '200': { description: ok } }
      trace:
        responses: { '200': { description: ok } }
      query:
        responses: { '200': { description: ok } }
      additionalOperations:
        CUSTOM:
          responses: { '200': { description: ok } }
    RefPi:
      $ref: '#/components/pathItems/SharedPi'
  parameters:
    ValidContentParam:
      name: vcp
      in: query
      content:
        application/x-www-form-urlencoded:
          schema: { type: object }
          encoding:
            a:
              contentType: text/plain
    BadParam: "not_a_param"
  requestBodies:
    ChainBody:
      $ref: '#/components/requestBodies/TargetBody'
    TargetBody:
      content:
        application/jsonl:
          itemSchema: { type: string }
        application/x-ndjson:
          itemSchema: { type: string }
        application/json-seq:
          itemSchema: { type: string }
        application/geo+json-seq:
          itemSchema: { type: string }
        application/vnd.custom+ndjson:
          itemSchema: { type: string }
        application/vnd.custom+json-seq:
          itemSchema: { type: string }
    CycleBody:
      $ref: '#/components/requestBodies/CycleBody'
    BadBody: "not_a_body"
  responses:
    ChainResp:
      $ref: '#/components/responses/TargetResp'
    TargetResp:
      description: target
      headers:
        X-H:
          style: simple
          content:
            text/plain:
              schema: { type: string }
    CycleResp:
      $ref: '#/components/responses/CycleResp'
  headers:
    ChainHdr:
      $ref: '#/components/headers/TargetHdr'
    TargetHdr:
      schema: { type: string }
    CycleHdr:
      $ref: '#/components/headers/CycleHdr'
  links:
    ValidLink:
      operationRef: /ref-path/1
      server:
        url: https://example.com
  examples:
    RefEx:
      $ref: '#/components/examples/ValEx'
    ValEx:
      value: 1
  mediaTypes:
    M1:
      schema: { type: string }
      examples:
        e1:
          value: "ok"
  securitySchemes:
    ApiKeyH:
      type: apiKey
      name: X-Key
      in: header
    ApiKeyQ:
      type: apiKey
      name: q_key
      in: query
    ApiKeyC:
      type: apiKey
      name: c_key
      in: cookie
    HttpBearer:
      type: http
      scheme: bearer
    OAuthFull:
      type: oauth2
      oauth2MetadataUrl: https://example.com/.well-known/oauth
      flows:
        implicit:
          authorizationUrl: https://example.com/auth
          refreshUrl: https://example.com/refresh
          scopes: {}
        password:
          tokenUrl: https://example.com/token
          scopes: {}
        clientCredentials:
          tokenUrl: https://example.com/token
          scopes: {}
        authorizationCode:
          authorizationUrl: https://example.com/auth
          tokenUrl: https://example.com/token
          scopes: {}
        deviceAuthorization:
          deviceAuthorizationUrl: https://example.com/device
          tokenUrl: https://example.com/token
          scopes: {}
    Oidc:
      type: openIdConnect
      openIdConnectUrl: https://example.com/.well-known/openid
    OAuthNoDevice:
      type: oauth2
      flows: {}
    Mtls:
      type: mutualTLS
    RefScheme:
      $ref: '#/components/securitySchemes/Mtls'
    RefUnresolved:
      $ref: '#/components/securitySchemes/Missing'
  schemas:
    ArrMeta:
      allOf:
        - type: string
          externalDocs:
            url: https://example.com/s-doc
          xml:
            nodeType: element
            name: item
            namespace: https://example.com/ns
            prefix: ex
        - type: string
          xml:
            attribute: false
            wrapped: true
"#;
        let full_doc: ShimOpenApi = serde_yaml::from_str(valid_full_yaml).expect("full_doc");
        validate_openapi_root(&full_doc).expect("valid full_doc");
        validate_component_keys(full_doc.components.as_ref().expect("comps")).expect("keys");

        let valid_sw2_yaml = r#"
swagger: "2.0"
info:
  title: Sw2
  version: "1.0"
  contact: {}
  license:
    name: Apache
    url: https://example.com/lic
securityDefinitions:
  BasicAuth:
    type: basic
  OAuthImp:
    type: oauth2
    flow: implicit
    authorizationUrl: https://example.com/auth
  OAuthPwd:
    type: oauth2
    flow: password
    tokenUrl: https://example.com/token
  OAuthApp:
    type: oauth2
    flow: application
    tokenUrl: https://example.com/token
  OAuthCode:
    type: oauth2
    flow: accessCode
    authorizationUrl: https://example.com/auth
    tokenUrl: https://example.com/token
security:
  - BasicAuth: []
paths:
  /p:
    get:
      parameters:
        - name: q
          in: query
          type: string
      responses:
        '200': { description: ok }
"#;
        let sw2_doc: ShimOpenApi = serde_yaml::from_str(valid_sw2_yaml).expect("sw2_doc");
        validate_openapi_root(&sw2_doc).expect("valid sw2_doc");

        let error_yamls = [
            "openapi: 3.2.0\npaths: {}\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\n",
            "openapi: 3.2.0\n$self: '   '\ninfo: { title: T, version: '1.0' }\npaths: {}\n",
            "openapi: 3.2.0\njsonSchemaDialect: 'http://[::1'\ninfo: { title: T, version: '1.0' }\npaths: {}\n",
            "openapi: 3.2.0\ninfo: { title: '  ', version: '1.0' }\npaths: {}\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '  ' }\npaths: {}\n",
            "openapi: 3.2.0\ninfo:\n  title: T\n  version: '1.0'\n  contact: { url: 'bad url' }\npaths: {}\n",
            "openapi: 3.2.0\ninfo:\n  title: T\n  version: '1.0'\n  contact: { email: 'a b@c.com' }\npaths: {}\n",
            "openapi: 3.2.0\ninfo:\n  title: T\n  version: '1.0'\n  contact: { email: '' }\npaths: {}\n",
            "openapi: 3.2.0\ninfo:\n  title: T\n  version: '1.0'\n  contact: { email: 'a@b@c.com' }\npaths: {}\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\npaths:\n  /p:\n    get:\n      responses:\n        '200':\n          $ref: '#/components/responses/EmptyDescResp'\ncomponents:\n  responses:\n    EmptyDescResp:\n      description: '  '\n",
            "openapi: 3.2.0\ninfo:\n  title: T\n  version: '1.0'\n  license: { name: '  ' }\npaths: {}\n",
            "openapi: 3.2.0\ninfo:\n  title: T\n  version: '1.0'\n  license: { name: MIT, url: 'bad url' }\npaths: {}\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\nexternalDocs: { url: 'bad url' }\npaths: {}\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ntags:\n  - name: t1\n    externalDocs: { url: 'bad url' }\npaths: {}\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  securitySchemes:\n    K:\n      type: apiKey\n      name: ' '\n      in: header\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  securitySchemes:\n    K:\n      type: apiKey\n      name: k\n      in: bad_in\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  securitySchemes:\n    H:\n      type: http\n      scheme: ' '\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  securitySchemes:\n    O:\n      type: oauth2\n      oauth2MetadataUrl: http://insecure.example.com\n      flows: {}\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  securitySchemes:\n    O:\n      type: oauth2\n      flows:\n        implicit: {}\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  securitySchemes:\n    O:\n      type: oauth2\n      flows:\n        password: {}\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\npaths:\n  /p:\n    post:\n      requestBody:\n        content:\n          multipart/mixed:\n            encoding: {}\n            prefixEncoding: []\n      responses:\n        '200': { description: ok }\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\nservers:\n  - url: https://{undef}.example.com\n    variables:\n      other: { default: x }\npaths: {}\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  securitySchemes:\n    O:\n      type: oauth2\n      flows:\n        deviceAuthorization:\n          tokenUrl: https://example.com/token\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  securitySchemes:\n    O:\n      type: oauth2\n      flows:\n        implicit:\n          authorizationUrl: https://example.com/auth\n          refreshUrl: 'not a url'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  securitySchemes:\n    Oidc:\n      type: openIdConnect\n      openIdConnectUrl: http://insecure.example.com\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  securitySchemes:\n    B:\n      type: basic\n",
            "swagger: '2.0'\ninfo: { title: T, version: '1.0' }\nsecurityDefinitions:\n  O:\n    type: oauth2\n",
            "swagger: '2.0'\ninfo: { title: T, version: '1.0' }\nsecurityDefinitions:\n  O:\n    type: oauth2\n    flow: implicit\n",
            "swagger: '2.0'\ninfo: { title: T, version: '1.0' }\nsecurityDefinitions:\n  O:\n    type: oauth2\n    flow: password\n",
            "swagger: '2.0'\ninfo: { title: T, version: '1.0' }\nsecurityDefinitions:\n  O:\n    type: oauth2\n    flow: accessCode\n",
            "swagger: '2.0'\ninfo: { title: T, version: '1.0' }\nsecurityDefinitions:\n  O:\n    type: oauth2\n    flow: accessCode\n    authorizationUrl: https://example.com/auth\n",
            "swagger: '2.0'\ninfo: { title: T, version: '1.0' }\nsecurityDefinitions:\n  O:\n    type: oauth2\n    flow: unknown_flow\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0', license: { name: Apache } }\nsecurity:\n  - 'not_an_object'\npaths: {}\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\nsecurity:\n  - 'http://[::1': []\npaths: {}\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\npaths:\n  /p:\n    get:\n      parameters:\n        - name: qs\n          in: querystring\n          schema: { type: string }\n      responses:\n        '200': { description: ok }\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\npaths:\n  /p:\n    get:\n      parameters:\n        - name: q\n          in: query\n          content:\n            application/json: {}\n            text/plain: {}\n      responses:\n        '200': { description: ok }\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  headers:\n    H1: 'not_an_obj'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  headers:\n    H1: { name: bad, schema: { type: string } }\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  headers:\n    H1: { schema: { type: string }, content: { 'text/plain': {} } }\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  headers:\n    H1: {}\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  headers:\n    H1: { content: 'not_an_obj' }\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  headers:\n    H1: { content: {} }\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  headers:\n    H1:\n      content:\n        text/plain:\n          examples: 'not_an_obj'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  examples: 'not_an_obj'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  examples:\n    E1: 'not_an_obj'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  examples:\n    E1:\n      serializedValue: 'a'\n      externalValue: 'b'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  mediaTypes:\n    M1: 'not_an_obj'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  mediaTypes:\n    M1:\n      examples: 'not_an_obj'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\npaths:\n  /p:\n    get:\n      responses:\n        '200':\n          description: ok\n          links: 'not_an_obj'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\npaths:\n  /p:\n    get:\n      responses:\n        '200':\n          description: ok\n          links:\n            L1: 'not_an_obj'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\npaths:\n  /p:\n    get:\n      responses:\n        '200':\n          description: ok\n          links:\n            L1:\n              $ref: '#/components/links/Missing'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\npaths:\n  /p:\n    get:\n      responses:\n        '200':\n          description: ok\n          links:\n            L1:\n              operationId: op1\n              server: 'not_a_server'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\npaths:\n  no_slash:\n    get:\n      responses:\n        '200': { description: ok }\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\npaths:\n  /a/{x}:\n    parameters:\n      - { name: x, in: path, required: true, schema: { type: string } }\n    get:\n      responses: { '200': { description: ok } }\n  /a/{y}:\n    parameters:\n      - { name: y, in: path, required: true, schema: { type: string } }\n    get:\n      responses: { '200': { description: ok } }\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  schemas: 'not_an_obj'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  schemas:\n    S1:\n      externalDocs: 'not_an_obj'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  schemas:\n    S1:\n      externalDocs: {}\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  schemas:\n    S1:\n      xml: 'not_an_obj'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  schemas:\n    S1:\n      xml: { nodeType: 123 }\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  schemas:\n    S1:\n      xml: { name: 123 }\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  schemas:\n    S1:\n      xml: { attribute: 123 }\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  schemas:\n    S1:\n      allOf:\n        - discriminator: 'not_an_obj'\n",
            "openapi: 3.2.0\ninfo: { title: T, version: '1.0' }\ncomponents:\n  schemas:\n    S1:\n      discriminator: { propertyName: '  ' }\n",
        ];

        for bad_yaml in error_yamls {
            let doc: ShimOpenApi = serde_yaml::from_str(bad_yaml).expect("parse bad_yaml");
            assert!(
                validate_openapi_root(&doc).is_err(),
                "expected error for:\n{bad_yaml}"
            );
        }

        for bad_comp_yaml in [
            "securitySchemes:\n  'bad key': { type: mutualTLS }\n",
            "pathItems:\n  'bad key': {}\n",
            "schemas:\n  'bad key': {}\n",
        ] {
            let comps: ShimComponents = serde_yaml::from_str(bad_comp_yaml).expect("bad_comp");
            assert!(validate_component_keys(&comps).is_err());
        }
        let comps_non_obj: ShimComponents =
            serde_yaml::from_str("schemas: 'not_an_obj'\n").expect("comps_non_obj");
        assert!(validate_component_keys(&comps_non_obj).is_ok());
        assert!(validate_encoding_headers(&serde_json::json!("not_obj"), "ctx", None).is_ok());
        assert!(validate_media_type_examples(
            &serde_json::json!({ "text/plain": "not_obj" }),
            "ctx"
        )
        .is_ok());
        assert!(validate_server_url("http://[::1", "ctx").is_err());
    }
}
