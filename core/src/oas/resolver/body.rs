#![deny(missing_docs)]

//! # Body Resolution
//!
//! Logic for extracting request body types from OpenAPI definitions.
//! Support for OAS 3.2 `encoding` definitions in multipart and url-encoded forms.

use crate::error::AppResult;
use crate::oas::models::{BodyFormat, RequestBodyDefinition};
use crate::oas::resolver::types::map_schema_to_rust_type;
use std::collections::HashMap;
use utoipa::openapi::encoding::Encoding;
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

    // 1. JSON
    if let Some(media) = content.get("application/json") {
        if let Some(schema_ref) = &media.schema {
            let type_str = map_schema_to_rust_type(schema_ref, true)?;
            return Ok(Some(RequestBodyDefinition {
                ty: type_str,
                format: BodyFormat::Json,
                encoding: None,
            }));
        }
    }

    // 2. Form URL Encoded
    if let Some(media) = content.get("application/x-www-form-urlencoded") {
        if let Some(schema_ref) = &media.schema {
            let type_str = map_schema_to_rust_type(schema_ref, true)?;
            let encoding = extract_encoding_map(&media.encoding);

            return Ok(Some(RequestBodyDefinition {
                ty: type_str,
                format: BodyFormat::Form,
                encoding,
            }));
        }
    }

    // 3. Multipart
    // OAS 3.2 allows multipart/form-data, multipart/mixed, etc.
    // We check for any key starting with "multipart/"
    if let Some((_, media)) = content.iter().find(|(k, _)| k.starts_with("multipart/")) {
        let type_str = if let Some(schema_ref) = &media.schema {
            map_schema_to_rust_type(schema_ref, true)?
        } else {
            "Multipart".to_string()
        };

        let encoding = extract_encoding_map(&media.encoding);

        return Ok(Some(RequestBodyDefinition {
            ty: type_str,
            format: BodyFormat::Multipart,
            encoding,
        }));
    }

    Ok(None)
}

/// Helper to extract encoding map (property -> content-type).
fn extract_encoding_map(
    encoding: &std::collections::BTreeMap<String, Encoding>,
) -> Option<HashMap<String, String>> {
    if encoding.is_empty() {
        return None;
    }

    let mut map = HashMap::new();
    for (prop, enc) in encoding {
        // According to OAS 3.2: contentType is string.
        // We capture it to allow specific part handling in Strategies.
        if let Some(ct) = &enc.content_type {
            map.insert(prop.clone(), ct.clone());
        }
    }

    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::openapi::encoding::Encoding;
    use utoipa::openapi::request_body::RequestBodyBuilder;
    use utoipa::openapi::Content;

    #[test]
    fn test_extract_json_body() {
        let body = RequestBodyBuilder::new()
            .content(
                "application/json",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/User",
                )))),
            )
            .build();

        let def = extract_request_body_type(&RefOr::T(body)).unwrap().unwrap();
        assert_eq!(def.ty, "User");
        assert_eq!(def.format, BodyFormat::Json);
        assert!(def.encoding.is_none());
    }

    #[test]
    fn test_extract_multipart_with_encoding() {
        // Encoding::builder().content_type(...) takes Option<String>
        let png_encoding = Encoding::builder()
            .content_type(Some("image/png".to_string()))
            .build();
        let json_encoding = Encoding::builder()
            .content_type(Some("application/json".to_string()))
            .build();

        // ContentBuilder::encoding takes (name, encoding) one by one
        let media = Content::builder()
            .schema(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                "#/components/schemas/Upload",
            ))))
            .encoding("profileImage", png_encoding)
            .encoding("metadata", json_encoding)
            .build();

        let body = RequestBodyBuilder::new()
            .content("multipart/form-data", media)
            .build();

        let def = extract_request_body_type(&RefOr::T(body)).unwrap().unwrap();

        assert_eq!(def.ty, "Upload");
        assert_eq!(def.format, BodyFormat::Multipart);

        let enc = def.encoding.unwrap();
        assert_eq!(enc.get("profileImage"), Some(&"image/png".to_string()));
        assert_eq!(enc.get("metadata"), Some(&"application/json".to_string()));
    }

    #[test]
    fn test_extract_form_no_encoding() {
        let request_body = RequestBodyBuilder::new()
            .content(
                "application/x-www-form-urlencoded",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/Login",
                )))),
            )
            .build();

        let def = extract_request_body_type(&RefOr::T(request_body))
            .unwrap()
            .unwrap();
        assert_eq!(def.ty, "Login");
        assert_eq!(def.format, BodyFormat::Form);
        assert!(def.encoding.is_none());
    }
}
