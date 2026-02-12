#![deny(missing_docs)]

//! # Extractor Assembly
//!
//! Logic for assembling the function signature of a handler by
//! mapping route parameters (Path, Query, etc.) to backend-specific extractors.

use crate::error::AppResult;
use crate::handler_generator::parsing::{
    extract_path_vars, find_param_type, to_pascal_case, to_snake_case,
};
use crate::oas::{BodyFormat, ParamSource, ParsedRoute};
use crate::strategies::BackendStrategy;
use std::collections::HashSet;

/// Represents a generated query parameter struct definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GeneratedQueryStruct {
    /// The Rust type name of the query struct.
    pub name: String,
    /// The Rust source code defining the struct.
    pub code: String,
}

/// Generates a query parameter struct definition for a route when query params are present.
pub(crate) fn generate_query_struct(route: &ParsedRoute) -> Option<GeneratedQueryStruct> {
    let mut params: Vec<_> = route
        .params
        .iter()
        .filter(|p| p.source == ParamSource::Query)
        .collect();

    if params.is_empty() {
        return None;
    }

    let struct_name = query_struct_name(route);
    let mut used_names = HashSet::new();
    let mut code = String::new();

    code.push_str(&format!(
        "/// Query parameters for `{}`.\n",
        route.handler_name
    ));
    code.push_str("#[derive(Debug, Clone, Deserialize)]\n");
    code.push_str(&format!("pub struct {} {{\n", struct_name));

    params.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
    for param in params {
        let base_name = sanitize_query_field_name(&param.name);
        let field_name = dedupe_field_name(base_name, &mut used_names);
        let needs_rename = field_name != param.name.as_str();

        if let Some(desc) = param.description.as_deref() {
            for line in desc.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                code.push_str(&format!("    /// {}\n", line));
            }
        } else {
            code.push_str(&format!(
                "    /// Query parameter `{}`.\n",
                param.name.as_str()
            ));
        }
        if needs_rename {
            code.push_str(&format!(
                "    #[serde(rename = \"{}\")]\n",
                escape_rust_string(&param.name)
            ));
        }
        code.push_str(&format!("    pub {}: {},\n", field_name, param.ty.as_str()));
    }

    code.push_str("}\n");

    Some(GeneratedQueryStruct {
        name: struct_name,
        code,
    })
}

fn query_struct_name(route: &ParsedRoute) -> String {
    format!("{}Query", to_pascal_case(&route.handler_name))
}

fn sanitize_query_field_name(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_underscore = false;

    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }

    if out.is_empty() {
        out.push_str("param");
    }

    if out
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        out.insert(0, '_');
    }

    if is_rust_keyword(&out) {
        out.push('_');
    }

    out
}

fn dedupe_field_name(base: String, used: &mut HashSet<String>) -> String {
    if used.insert(base.clone()) {
        return base;
    }
    let mut idx = 2;
    loop {
        let candidate = format!("{}_{}", base, idx);
        if used.insert(candidate.clone()) {
            return candidate;
        }
        idx += 1;
    }
}

fn escape_rust_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

fn is_rust_keyword(ident: &str) -> bool {
    matches!(
        ident,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
    )
}

fn build_handler_docs(route: &ParsedRoute) -> String {
    let mut docs = String::new();

    if let Some(summary) = &route.summary {
        docs.push_str(&format!("/// {}\n", summary));
    }
    if let Some(description) = &route.description {
        if !docs.is_empty() {
            docs.push_str("///\n");
        }
        for line in description.lines() {
            docs.push_str(&format!("/// {}\n", line));
        }
    }
    if let Some(ext) = &route.external_docs {
        if !docs.is_empty() {
            docs.push_str("///\n");
        }
        let label = ext.description.as_deref().unwrap_or("See also");
        docs.push_str(&format!("/// {}: <{}>\n", label, ext.url));
    }

    docs
}

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
            let query_struct = query_struct_name(route);
            args.push(format!(
                "query: {}",
                strategy.typed_query_extractor(&query_struct)
            ));
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
        let mut extractor = match def.format {
            BodyFormat::Json => strategy.body_extractor(&def.ty),
            BodyFormat::Form => strategy.form_extractor(&def.ty),
            // Pass the type to the multipart extractor
            BodyFormat::Multipart => strategy.multipart_extractor(&def.ty),
            BodyFormat::Text => strategy.text_extractor(&def.ty),
            BodyFormat::Binary => strategy.bytes_extractor(&def.ty),
        };
        if !def.required {
            extractor = format!("Option<{}>", extractor);
        }
        args.push(format!("body: {}", extractor));
    }

    let security_arg = strategy.security_extractor(&route.security);
    if !security_arg.is_empty() {
        args.push(security_arg);
    }

    let mut code = String::new();
    let docs = build_handler_docs(route);
    if !docs.is_empty() {
        code.push_str(&docs);
    }
    if route.deprecated {
        code.push_str("#[deprecated]\n");
    }

    let signature = strategy.handler_signature(
        &route.handler_name,
        &args,
        route.response_type.as_deref(),
        &route.response_headers,
        route.response_links.as_deref(),
    );
    code.push_str(&signature);

    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler_generator::builder::update_handler_module;
    use crate::oas::models::{ResponseHeader, RouteKind, SecurityRequirement};
    use crate::oas::{BodyFormat, RequestBodyDefinition, RouteParam};
    use crate::parser::ParsedExternalDocs;
    use crate::strategies::ActixStrategy;

    #[test]
    fn test_single_path_param() {
        let route = ParsedRoute {
            path: "/users/{id}".into(),
            summary: None,
            description: None,
            base_path: None,
            method: "GET".into(),
            handler_name: "get_user".into(),
            params: vec![RouteParam {
                name: "id".into(),
                description: None,
                source: ParamSource::Path,
                ty: "Uuid".into(),
                content_media_type: None,
                style: None,
                explode: false,
                deprecated: false,
                allow_empty_value: false,
                allow_reserved: false,
                example: None,
            }],
            request_body: None,
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
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
            summary: None,
            description: None,
            base_path: None,
            method: "POST".into(),
            handler_name: "search".into(),
            params: vec![RouteParam {
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
            }],
            request_body: Some(RequestBodyDefinition {
                ty: "SearchFilter".into(),
                description: None,
                media_type: "application/json".into(),
                format: BodyFormat::Json,
                required: true,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
                example: None,
            }),
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        assert!(code.contains("pub struct SearchQuery"));
        assert!(code.contains("query: web::Query<SearchQuery>"));
        assert!(code.contains("body: web::Json<SearchFilter>"));
    }

    #[test]
    fn test_multipart_extractor_generates_typed() {
        let route = ParsedRoute {
            path: "/upload".into(),
            summary: None,
            description: None,
            base_path: None,
            method: "POST".into(),
            handler_name: "upload_file".into(),
            params: vec![],
            request_body: Some(RequestBodyDefinition {
                ty: "UploadForm".into(),
                description: None,
                media_type: "multipart/form-data".into(),
                format: BodyFormat::Multipart,
                required: true,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
                example: None,
            }),
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        assert!(code.contains("body: actix_multipart::form::MultipartForm<UploadForm>"));
    }

    #[test]
    fn test_text_and_binary_extractors() {
        let text_route = ParsedRoute {
            path: "/text".into(),
            summary: None,
            description: None,
            base_path: None,
            method: "POST".into(),
            handler_name: "accept_text".into(),
            params: vec![],
            request_body: Some(RequestBodyDefinition {
                ty: "String".into(),
                description: None,
                media_type: "text/plain".into(),
                format: BodyFormat::Text,
                required: true,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
                example: None,
            }),
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };

        let binary_route = ParsedRoute {
            path: "/bin".into(),
            summary: None,
            description: None,
            base_path: None,
            method: "POST".into(),
            handler_name: "accept_binary".into(),
            params: vec![],
            request_body: Some(RequestBodyDefinition {
                ty: "Vec<u8>".into(),
                description: None,
                media_type: "application/octet-stream".into(),
                format: BodyFormat::Binary,
                required: true,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
                example: None,
            }),
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[text_route, binary_route], &strategy).unwrap();
        assert!(code.contains("body: String"));
        assert!(code.contains("body: web::Bytes"));
    }

    #[test]
    fn test_optional_body_extractor() {
        let route = ParsedRoute {
            path: "/optional".into(),
            summary: None,
            description: None,
            base_path: None,
            method: "POST".into(),
            handler_name: "optional_body".into(),
            params: vec![],
            request_body: Some(RequestBodyDefinition {
                ty: "Payload".into(),
                description: None,
                media_type: "application/json".into(),
                format: BodyFormat::Json,
                required: false,
                encoding: None,
                prefix_encoding: None,
                item_encoding: None,
                example: None,
            }),
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        assert!(code.contains("body: Option<web::Json<Payload>>"));
    }

    #[test]
    fn test_oas_3_2_querystring_extractor() {
        let route = ParsedRoute {
            path: "/raw".into(),
            summary: None,
            description: None,
            base_path: None,
            method: "GET".into(),
            handler_name: "raw_search".into(),
            params: vec![RouteParam {
                name: "filter".into(),
                description: None,
                source: ParamSource::QueryString,
                ty: "FilterStruct".into(),
                content_media_type: None,
                style: None,
                explode: false,
                deprecated: false,
                allow_empty_value: false,
                allow_reserved: false,
                example: None,
            }],
            request_body: None,
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
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
            summary: None,
            description: None,
            base_path: None,
            method: "POST".into(),
            handler_name: "secure_ops".into(),
            params: vec![],
            request_body: None,
            security: vec![SecurityRequirement {
                scheme_name: "ApiKey".into(),
                scopes: vec![],
                scheme: None, // Simplified for this test file context
            }],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };
        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();

        // UPDATED EXPECTATION: Variable is now `api_key` due to smarter naming
        assert!(code.contains("api_key: web::ReqData<security::ApiKey>"));
    }

    #[test]
    fn test_extractor_passes_headers_info() {
        let route = ParsedRoute {
            path: "/headers".into(),
            summary: None,
            description: None,
            base_path: None,
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
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };
        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        assert!(code.contains("-> actix_web::Result<HttpResponse>"));
    }

    #[test]
    fn test_handler_docs_and_deprecated() {
        let route = ParsedRoute {
            path: "/doc".into(),
            summary: Some("Short summary".into()),
            description: Some("Longer description.".into()),
            base_path: None,
            method: "GET".into(),
            handler_name: "doc_route".into(),
            params: vec![],
            request_body: None,
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: true,
            external_docs: Some(ParsedExternalDocs {
                url: "https://example.com/docs".into(),
                description: Some("Docs".into()),
            }),
        };

        let strategy = ActixStrategy;
        let code = update_handler_module("", &[route], &strategy).unwrap();
        assert!(code.contains("/// Short summary"));
        assert!(code.contains("/// Longer description."));
        assert!(code.contains("/// Docs: <https://example.com/docs>"));
        assert!(code.contains("#[deprecated]"));
    }

    #[test]
    fn test_generate_query_struct_renames_and_dedupes() {
        let route = ParsedRoute {
            path: "/q".into(),
            summary: None,
            description: None,
            base_path: None,
            method: "GET".into(),
            handler_name: "list_items".into(),
            params: vec![
                RouteParam {
                    name: "user-id".into(),
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
                },
                RouteParam {
                    name: "user_id".into(),
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
                },
            ],
            request_body: None,
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };

        let generated = generate_query_struct(&route).unwrap();
        assert!(generated.code.contains("pub struct ListItemsQuery"));
        assert!(generated.code.contains("#[serde(rename = \"user-id\")]"));
        assert!(generated.code.contains("pub user_id: String"));
        assert!(generated.code.contains("pub user_id_2: String"));
    }

    #[test]
    fn test_generate_query_struct_includes_param_description() {
        let route = ParsedRoute {
            path: "/search".into(),
            summary: None,
            description: None,
            base_path: None,
            method: "GET".into(),
            handler_name: "search".into(),
            params: vec![RouteParam {
                name: "status".into(),
                description: Some("Filter results by status.".into()),
                source: ParamSource::Query,
                ty: "String".into(),
                content_media_type: None,
                style: None,
                explode: false,
                deprecated: false,
                allow_empty_value: false,
                allow_reserved: false,
                example: None,
            }],
            request_body: None,
            security: vec![],
            response_type: None,
            response_headers: vec![],
            response_links: None,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
        };

        let generated = generate_query_struct(&route).unwrap();
        assert!(generated.code.contains("/// Filter results by status."));
        assert!(generated.code.contains("pub status: String"));
    }
}
