#![deny(missing_docs)]

//! # OpenAPI Validation
//!
//! Helper functions that enforce structural requirements from the OpenAPI 3.2.0
//! specification before deeper parsing or code generation.

use crate::error::{AppError, AppResult};
use crate::oas::routes::shims::{ShimComponents, ShimOpenApi, ShimPathItem, ShimServer};
use regex::Regex;
use std::collections::{BTreeMap, HashMap, HashSet};
use url::Url;

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

    validate_tags_unique(openapi)?;
    validate_servers(openapi)?;

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

fn validate_server_list(servers: &[ShimServer], context: &str) -> AppResult<()> {
    for (idx, server) in servers.iter().enumerate() {
        let server_context = format!("{}[{}]", context, idx);
        validate_server(server, &server_context)?;
    }
    Ok(())
}

fn validate_server(server: &ShimServer, context: &str) -> AppResult<()> {
    validate_server_url(&server.url, context)?;

    if let Some(vars) = &server.variables {
        validate_server_variables(&server.url, vars, context)?;
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
    for (name, var) in vars {
        if let Some(enum_vals) = &var.enum_values {
            if !enum_vals.contains(&var.default) {
                return Err(AppError::General(format!(
                    "Server variable '{}' in {} has default '{}' not in enum",
                    name, context, var.default
                )));
            }
        }

        let placeholder = format!("{{{}}}", name);
        let occurrences = url.matches(&placeholder).count();
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
}
