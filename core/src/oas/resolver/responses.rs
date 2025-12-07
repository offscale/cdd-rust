#![deny(missing_docs)]

//! # Response Resolution
//!
//! Logic for resolving OpenAPI Responses into Rust types.

use crate::error::AppResult;
use crate::oas::resolver::types::map_schema_to_rust_type;
use crate::oas::routes::shims::ShimComponents;
use utoipa::openapi::{RefOr, Responses};

/// Extracts the success response type (200 OK or 201 Created).
pub fn extract_response_success_type(
    responses: &Responses,
    components: Option<&ShimComponents>,
) -> AppResult<Option<String>> {
    // Check 200 then 201
    let success = responses
        .responses
        .get("200")
        .or_else(|| responses.responses.get("201"));

    if let Some(resp_item) = success {
        let response = match resp_item {
            RefOr::T(val) => Some(val.clone()),
            RefOr::Ref(r) => resolve_response_from_components(r, components),
        };

        if let Some(r) = response {
            if let Some(media) = r.content.get("application/json") {
                if let Some(schema) = &media.schema {
                    let ty = map_schema_to_rust_type(schema, true)?;
                    return Ok(Some(ty));
                }
            }
        }
    }

    Ok(None)
}

fn resolve_response_from_components(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
) -> Option<utoipa::openapi::Response> {
    let ref_name = r.ref_location.split('/').next_back()?;
    if let Some(comps) = components {
        // Access via 'extra' flattening for generic items
        if let Some(resp_json) = comps.extra.get("responses").and_then(|r| r.get(ref_name)) {
            if let Ok(resp) = serde_json::from_value::<utoipa::openapi::Response>(resp_json.clone())
            {
                return Some(resp);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::openapi::{Content, ResponseBuilder};

    #[test]
    fn test_extract_inline_response() {
        let response = ResponseBuilder::new()
            .description("Inline")
            .content(
                "application/json",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/User",
                )))),
            )
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let ty = extract_response_success_type(&responses, None)
            .unwrap()
            .unwrap();
        assert_eq!(ty, "User");
    }
}
