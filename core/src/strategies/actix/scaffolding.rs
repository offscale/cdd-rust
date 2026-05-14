#![deny(missing_docs)]
#![cfg(not(tarpaulin_include))]

//! # Scaffolding
//!
//! Logic for generating handler signatures, imports, and function bodies.

use crate::strategies::actix::links::generate_link_construction;

/// Returns standard imports for Actix handlers.
pub fn handler_imports() -> String {
    let mut imports = String::new();
    imports.push_str("use actix_web::{web, HttpResponse, Responder};\n");
    imports.push_str("use actix_multipart::Multipart;\n");
    imports.push_str("use serde::Deserialize;\n");
    imports.push_str("use serde_json::Value;\n");
    imports.push_str("use uuid::Uuid;\n");
    imports.push_str("use chrono::{DateTime, Utc, NaiveDate, NaiveDateTime};\n");
    imports
}

/// Generates the handler function signature and body scaffold.
pub fn handler_signature(route: &crate::openapi::parse::ParsedRoute, args: &[String]) -> String {
    let func_name = &route.handler_name;
    let response_headers = &route.response_headers;
    let response_type = route.response_type.as_deref();
    let response_links = route.response_links.as_deref();

    let args_str = args.join(", ");

    let has_headers = !response_headers.is_empty();
    let has_links = response_links.map(|l| !l.is_empty()).unwrap_or(false);

    let return_type = if has_headers || has_links {
        "actix_web::Result<HttpResponse>".to_string()
    } else if let Some(rt) = response_type {
        format!("actix_web::Result<web::Json<{}>>", rt)
    } else {
        "impl Responder".to_string()
    };

    let mut body = String::new();

    // Generate logic for links (Runtime Variables)
    if has_links {
        if let Some(links) = response_links {
            body.push_str("    // -- Generated Links --\n");
            for link in links {
                let (code, var_name) = generate_link_construction(link);
                body.push_str(&code);
                // Append header using the constructed variable
                body.push_str(&format!(
                    "    // .append_header((\"Link\", format!(\"<{{}}>; rel=\\\"{}\\\"\", {})))\n",
                    link.name, var_name
                ));
                if let Some(request_body) = &link.request_body {
                    body.push_str(&format!("    // Link requestBody: {:?}\n", request_body));
                }
                if let Some(server_url) = &link.server_url {
                    body.push_str(&format!("    // Link server override: {}\n", server_url));
                }
            }
            body.push_str("    // ---------------------\n\n");
        }
    }

    if has_headers {
        body.push_str("    // Required Response Headers:\n");
        for h in response_headers {
            let desc = h.description.as_deref().unwrap_or("No description");
            body.push_str(&format!("    // - {}: {} ({})\n", h.name, h.ty, desc));
        }
    }

    if has_headers || has_links {
        // Use generic [Status] placeholder in comment since we support 2XX/3XX/default
        body.push_str("    // Example:\n    // HttpResponse::[Status]()\n");
        if has_links {
            body.push_str("    //     .append_header((\"Link\", ...))\n");
        }
        if response_type.is_some() {
            body.push_str("    //     .json(body)\n");
        } else {
            body.push_str("    //     .finish()\n");
        }
    }

    body.push_str("    todo!()");

    format!(
        "pub async fn {}({}) -> {} {{\n{}\n}}\n",
        func_name, args_str, return_type, body
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::parse::models::{
        LinkParamValue, LinkRequestBody, ParsedLink, ResponseHeader, RouteKind, RuntimeExpression,
    };
    use crate::openapi::parse::ParsedRoute;
    use std::collections::{BTreeMap, HashMap};

    fn dummy_route(
        handler_name: &str,
        response_type: Option<&str>,
        headers: Vec<ResponseHeader>,
        links: Option<Vec<ParsedLink>>,
    ) -> ParsedRoute {
        ParsedRoute {
            path: "/".into(),
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
            method: "GET".into(),
            handler_name: handler_name.into(),
            operation_id: None,
            params: vec![],
            path_params: vec![],
            request_body: None,
            security: vec![],
            security_defined: false,
            response_type: response_type.map(|s| s.to_string()),
            response_status: None,
            response_summary: None,
            response_description: None,
            response_media_type: None,
            response_example: None,
            response_headers: headers,
            response_links: links,
            kind: RouteKind::Path,
            tags: vec![],
            callbacks: vec![],
            deprecated: false,
            external_docs: None,
            raw_request_body: None,
            raw_responses: None,
            extensions: BTreeMap::new(),
        }
    }

    #[test]
    fn test_handler_imports_contains_expected_use() {
        let imports = handler_imports();
        assert!(imports.contains("actix_web::{web, HttpResponse, Responder}"));
        assert!(imports.contains("use serde::Deserialize;"));
        assert!(imports.contains("chrono::{DateTime, Utc, NaiveDate, NaiveDateTime}"));
    }

    #[test]
    fn test_handler_signature_with_response_type() {
        let args = vec!["id: web::Path<Uuid>".to_string()];
        let route = dummy_route("get_user", Some("User"), vec![], None);
        let sig = handler_signature(&route, &args);
        assert!(sig.contains("pub async fn get_user"));
        assert!(sig.contains("-> actix_web::Result<web::Json<User>>"));
        assert!(sig.contains("todo!()"));
    }

    #[test]
    fn test_handler_signature_with_headers_and_links() {
        let headers = vec![ResponseHeader {
            name: "X-Rate-Limit".to_string(),
            description: Some("limit".to_string()),
            required: false,
            deprecated: false,
            style: None,
            explode: None,
            ty: "i32".to_string(),
            content_media_type: None,
            example: None,
            extensions: BTreeMap::new(),
        }];

        let mut params = HashMap::new();
        params.insert(
            "id".to_string(),
            LinkParamValue::Expression(RuntimeExpression::new("$response.body#/id")),
        );
        let links = vec![ParsedLink {
            name: "User".to_string(),
            description: None,
            operation_id: None,
            operation_ref: Some("/users/{id}".to_string()),
            resolved_operation_ref: None,
            parameters: params,
            request_body: Some(LinkRequestBody::Literal(serde_json::json!("payload"))),
            server: None,
            server_url: Some("https://api.example.com".to_string()),
        }];

        let route = dummy_route("get_user", Some("User"), headers, Some(links));
        let sig = handler_signature(&route, &[]);
        assert!(sig.contains("-> actix_web::Result<HttpResponse>"));
        assert!(sig.contains("Required Response Headers"));
        assert!(sig.contains("Generated Links"));
        assert!(sig.contains("todo!()"));
    }

    #[test]
    fn test_handler_signature_without_response_type_or_headers() {
        let route = dummy_route("ping", None, vec![], None);
        let sig = handler_signature(&route, &[]);
        assert!(sig.contains("-> impl Responder"));
    }
}
