#![deny(missing_docs)]

//! # Scaffolding
//!
//! Logic for generating handler signatures, imports, and function bodies.

use crate::oas::models::{ParsedLink, ResponseHeader};
use crate::strategies::actix::links::generate_link_construction;

/// Returns standard imports for Actix handlers.
pub fn handler_imports() -> String {
    let mut imports = String::new();
    imports.push_str("use actix_web::{web, HttpResponse, Responder};\n");
    imports.push_str("use actix_multipart::Multipart;\n");
    imports.push_str("use serde_json::Value;\n");
    imports.push_str("use uuid::Uuid;\n");
    imports.push_str("use chrono::{DateTime, Utc, NaiveDate, NaiveDateTime};\n");
    imports
}

/// Generates the handler function signature and body scaffold.
pub fn handler_signature(
    func_name: &str,
    args: &[String],
    response_type: Option<&str>,
    response_headers: &[ResponseHeader],
    response_links: Option<&[ParsedLink]>,
) -> String {
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
