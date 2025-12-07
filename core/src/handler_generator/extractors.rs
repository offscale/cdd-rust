#![deny(missing_docs)]

//! # Extractor Assembly
//!
//! Logic for assembling the function signature of a handler by
//! mapping route parameters (Path, Query, etc.) to backend-specific extractors.

use crate::error::AppResult;
use crate::handler_generator::parsing::{extract_path_vars, find_param_type, to_snake_case};
use crate::oas::{BodyFormat, ParamSource, ParsedRoute};
use crate::strategies::BackendStrategy;

/// Generates a single async handler function string.
pub(crate) fn generate_function(
    route: &ParsedRoute,
    strategy: &impl BackendStrategy,
) -> AppResult<String> {
    let mut args = Vec::new();

    // 1. Path Parameters
    let path_vars = extract_path_vars(&route.path);
    if !path_vars.is_empty() {
        let types: Vec<String> = path_vars
            .iter()
            .map(|name| {
                find_param_type(route, name, ParamSource::Path)
                    .unwrap_or_else(|| "String".to_string())
            })
            .collect();

        let type_signature = strategy.path_extractor(&types);

        if types.len() == 1 {
            let var_name = to_snake_case(&path_vars[0]);
            args.push(format!("{}: {}", var_name, type_signature));
        } else {
            args.push(format!("path: {}", type_signature));
        }
    }

    if let Some(qs_param) = route
        .params
        .iter()
        .find(|p| p.source == ParamSource::QueryString)
    {
        let var_name = to_snake_case(&qs_param.name);
        args.push(format!(
            "{}: {}",
            var_name,
            strategy.query_string_extractor(&qs_param.ty)
        ));
    } else {
        let has_query = route.params.iter().any(|p| p.source == ParamSource::Query);
        if has_query {
            args.push(format!("query: {}", strategy.query_extractor()));
        }
    }

    for param in route
        .params
        .iter()
        .filter(|p| p.source == ParamSource::Header)
    {
        let var_name = to_snake_case(&param.name);
        let extractor_type = strategy.header_extractor(&param.ty);
        args.push(format!("{}: {}", var_name, extractor_type));
    }

    for param in route
        .params
        .iter()
        .filter(|p| p.source == ParamSource::Cookie)
    {
        let var_name = to_snake_case(&param.name);
        args.push(format!("{}: {}", var_name, strategy.cookie_extractor()));
    }

    if let Some(def) = &route.request_body {
        let extractor = match def.format {
            BodyFormat::Json => strategy.body_extractor(&def.ty),
            BodyFormat::Form => strategy.form_extractor(&def.ty),
            BodyFormat::Multipart => strategy.multipart_extractor(),
        };
        args.push(format!("body: {}", extractor));
    }

    let security_arg = strategy.security_extractor(&route.security);
    if !security_arg.is_empty() {
        args.push(security_arg);
    }

    let code = strategy.handler_signature(
        &route.handler_name,
        &args,
        route.response_type.as_deref(),
        &route.response_headers,
        route.response_links.as_deref(),
    );

    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler_generator::builder::update_handler_module;
    use crate::oas::models::{ResponseHeader, RouteKind, SecurityRequirement};
    use crate::oas::{BodyFormat, RequestBodyDefinition, RouteParam};
    use crate::strategies::ActixStrategy;

    #[test]
    fn test_single_path_param() {
        let route = ParsedRoute {
            path: "/users/{id}".into(),
            method: "GET".into(),
            handler_name: "get_user".into(),
            params: vec![RouteParam {
                name: "id".into(),
                source: ParamSource::Path,
                ty: "Uuid".into(),
                style: None,
                explode: false,
                allow_reserved: false,
            }],
            request_body: None,
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        assert!(code.contains("id: web::Path<Uuid>"));
    }

    #[test]
    fn test_query_and_body() {
        let route = ParsedRoute {
            path: "/search".into(),
            method: "POST".into(),
            handler_name: "search".into(),
            params: vec![RouteParam {
                name: "q".into(),
                source: ParamSource::Query,
                ty: "String".into(),
                style: None,
                explode: false,
                allow_reserved: false,
            }],
            request_body: Some(RequestBodyDefinition {
                ty: "SearchFilter".into(),
                format: BodyFormat::Json,
                encoding: None,
            }),
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        assert!(code.contains("query: web::Query<Value>"));
        assert!(code.contains("body: web::Json<SearchFilter>"));
    }

    #[test]
    fn test_oas_3_2_querystring_extractor() {
        let route = ParsedRoute {
            path: "/raw".into(),
            method: "GET".into(),
            handler_name: "raw_search".into(),
            params: vec![RouteParam {
                name: "filter".into(),
                source: ParamSource::QueryString,
                ty: "FilterStruct".into(),
                style: None,
                explode: false,
                allow_reserved: false,
            }],
            request_body: None,
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        assert!(code.contains("filter: web::Query<FilterStruct>"));
    }

    #[test]
    fn test_security_stub_gen() {
        let route = ParsedRoute {
            path: "/api".into(),
            method: "POST".into(),
            handler_name: "secure_ops".into(),
            params: vec![],
            request_body: None,
            security: vec![SecurityRequirement {
                scheme_name: "ApiKey".into(),
                scopes: vec![],
            }],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };
        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();

        assert!(code.contains("_auth: web::ReqData<security::ApiKey>"));
    }

    #[test]
    fn test_extractor_passes_headers_info() {
        let route = ParsedRoute {
            path: "/headers".into(),
            method: "GET".into(),
            handler_name: "get_headers".into(),
            params: vec![],
            request_body: None,
            security: vec![],
            response_type: Some("Body".to_string()),
            response_headers: vec![ResponseHeader {
                name: "X-Custom".to_string(),
                description: None,
                ty: "String".to_string(),
            }],
            response_links: None,
            kind: RouteKind::Path,
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };
        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        assert!(code.contains("-> actix_web::Result<HttpResponse>"));
    }
}
