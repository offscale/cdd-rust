#![deny(missing_docs)]

//! # Body Resolution
//!
//! Logic for extracting request body types from OpenAPI definitions.
//! Support for OAS 3.2 `encoding` definitions in multipart and url-encoded forms.

use crate::error::AppResult;
use crate::oas::models::{BodyFormat, EncodingInfo, RequestBodyDefinition};
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
            let encoding = extract_encoding_map(&media.encoding)?;

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

        let encoding = extract_encoding_map(&media.encoding)?;

        return Ok(Some(RequestBodyDefinition {
            ty: type_str,
            format: BodyFormat::Multipart,
            encoding,
        }));
    }

    Ok(None)
}

/// Helper to extract encoding map (property -> EncodingInfo).
fn extract_encoding_map(
    encoding: &std::collections::BTreeMap<String, Encoding>,
) -> AppResult<Option<HashMap<String, EncodingInfo>>> {
    if encoding.is_empty() {
        return Ok(None);
    }

    let mut map = HashMap::new();
    for (prop, enc) in encoding {
        // Extract headers if present
        let mut headers = HashMap::new();
        for (h_name, h_ref) in &enc.headers {
            let ty = map_schema_to_rust_type(&h_ref.schema, true)?;
            headers.insert(h_name.clone(), ty);
        }

        map.insert(
            prop.clone(),
            EncodingInfo {
                content_type: enc.content_type.clone(),
                headers,
            },
        );
    }

    if map.is_empty() {
        Ok(None)
    } else {
        Ok(Some(map))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::openapi::encoding::EncodingBuilder;
    use utoipa::openapi::header::HeaderBuilder;
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
    fn test_extract_multipart_with_encoding_and_headers() {
        // Encoding with Content-Type and Custom Header
        let png_encoding = EncodingBuilder::new()
            .content_type(Some("image/png".to_string()))
            .header(
                "X-Image-Id",
                HeaderBuilder::new()
                    .schema(RefOr::Ref(utoipa::openapi::Ref::new(
                        "#/components/schemas/Uuid",
                    )))
                    .build(),
            )
            .build();

        let json_encoding = EncodingBuilder::new()
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
        let profile = enc.get("profileImage").unwrap();
        assert_eq!(profile.content_type.as_deref(), Some("image/png"));
        assert_eq!(
            profile.headers.get("X-Image-Id").map(|s| s.as_str()),
            Some("Uuid")
        );

        let meta = enc.get("metadata").unwrap();
        assert_eq!(meta.content_type.as_deref(), Some("application/json"));
        assert!(meta.headers.is_empty());
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
