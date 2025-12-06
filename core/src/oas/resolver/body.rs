#![deny(missing_docs)]

//! # Body Resolution
//!
//! Logic for extracting request body types from OpenAPI definitions.

use crate::error::AppResult;
use crate::oas::models::{BodyFormat, RequestBodyDefinition};
use crate::oas::resolver::types::map_schema_to_rust_type;
use utoipa::openapi::request_body::RequestBody;
use utoipa::openapi::RefOr;

/// Extracts the request body type and format from the OpenAPI definition.
pub fn extract_request_body_type(
    body: &RefOr<RequestBody>,
) -> AppResult<Option<RequestBodyDefinition>> {
    let content = match body {
        RefOr::T(b) => &b.content,
        RefOr::Ref(_) => return Ok(None),
    };

    if let Some(media) = content.get("application/json") {
        if let Some(schema_ref) = &media.schema {
            let type_str = map_schema_to_rust_type(schema_ref, true)?;
            return Ok(Some(RequestBodyDefinition {
                ty: type_str,
                format: BodyFormat::Json,
            }));
        }
    }

    if let Some(media) = content.get("application/x-www-form-urlencoded") {
        if let Some(schema_ref) = &media.schema {
            let type_str = map_schema_to_rust_type(schema_ref, true)?;
            return Ok(Some(RequestBodyDefinition {
                ty: type_str,
                format: BodyFormat::Form,
            }));
        }
    }

    if let Some(media) = content.get("multipart/form-data") {
        let type_str = if let Some(schema_ref) = &media.schema {
            map_schema_to_rust_type(schema_ref, true)?
        } else {
            "Multipart".to_string()
        };

        return Ok(Some(RequestBodyDefinition {
            ty: type_str,
            format: BodyFormat::Multipart,
        }));
    }

    Ok(None)
}
