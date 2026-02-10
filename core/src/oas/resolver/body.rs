#![deny(missing_docs)]

//! # Body Resolution
//!
//! Logic for extracting request body types from OpenAPI definitions.
//! Support for OAS 3.2 `encoding` definitions in multipart and url-encoded forms.

use crate::error::AppResult;
use crate::oas::models::{BodyFormat, EncodingInfo, RequestBodyDefinition};
use crate::oas::ref_utils::extract_component_name;
use crate::oas::resolver::types::map_schema_to_rust_type;
use crate::oas::routes::shims::ShimComponents;
use std::collections::HashMap;
use utoipa::openapi::encoding::Encoding;
use utoipa::openapi::request_body::RequestBody;
use utoipa::openapi::RefOr;

/// Extracts the request body type and format from the OpenAPI definition.
///
/// Resolves `$ref` values against `components.requestBodies` when available, including
/// OAS 3.2 `$self`-qualified absolute references.
pub fn extract_request_body_type(
    body: &RefOr<RequestBody>,
    components: Option<&ShimComponents>,
) -> AppResult<Option<RequestBodyDefinition>> {
    let owned_body;
    let content = match body {
        RefOr::T(b) => &b.content,
        RefOr::Ref(r) => {
            if let Some(resolved) = resolve_request_body_from_components(r, components) {
                owned_body = resolved;
                &owned_body.content
            } else {
                return Ok(None);
            }
        }
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

fn resolve_request_body_from_components(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
) -> Option<RequestBody> {
    let (comps, self_uri) =
        components.map(|c| (c, c.extra.get("__self").and_then(|v| v.as_str())))?;
    let ref_name = extract_component_name(&r.ref_location, self_uri, "requestBodies")?;
    if let Some(body_json) = comps
        .extra
        .get("requestBodies")
        .and_then(|m| m.get(&ref_name))
    {
        if let Ok(body) = serde_json::from_value::<RequestBody>(body_json.clone()) {
            return Some(body);
        }
    }
    None
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
    use crate::oas::routes::shims::ShimComponents;
    use serde_json::json;
    use std::collections::BTreeMap;
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

        let def = extract_request_body_type(&RefOr::T(body), None).unwrap().unwrap();
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

        let def = extract_request_body_type(&RefOr::T(body), None).unwrap().unwrap();

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

        let def = extract_request_body_type(&RefOr::T(request_body), None)
            .unwrap()
            .unwrap();
        assert_eq!(def.ty, "Login");
        assert_eq!(def.format, BodyFormat::Form);
        assert!(def.encoding.is_none());
    }

    #[test]
    fn test_extract_request_body_ref_with_self() {
        let mut components = ShimComponents {
            security_schemes: None,
            path_items: None,
            extra: BTreeMap::new(),
        };
        components.extra.insert(
            "__self".to_string(),
            json!("https://example.com/openapi.yaml"),
        );
        components.extra.insert(
            "requestBodies".to_string(),
            json!({
                "CreateThing": {
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/Thing" }
                        }
                    }
                }
            }),
        );

        let body_ref = RefOr::Ref(utoipa::openapi::Ref::new(
            "https://example.com/openapi.yaml#/components/requestBodies/CreateThing",
        ));
        let def = extract_request_body_type(&body_ref, Some(&components))
            .unwrap()
            .unwrap();
        assert_eq!(def.ty, "Thing");
        assert_eq!(def.format, BodyFormat::Json);
    }
}
