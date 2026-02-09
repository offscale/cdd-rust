#![deny(missing_docs)]

//! # Response Resolution
//!
//! Logic for resolving OpenAPI Responses into Rust types.

use crate::error::AppResult;
use indexmap::IndexMap;
use crate::oas::models::{ParsedLink, ResponseHeader, RuntimeExpression};
use crate::oas::resolver::types::map_schema_to_rust_type;
use crate::oas::routes::shims::ShimComponents;
use utoipa::openapi::{Content, RefOr, Responses};

// Re-export specific structs if needed for external visibility
pub use crate::oas::models::ParsedLink as LinkModel;

/// The extracted details of a successful response.
pub struct ParsedResponseDetails {
    /// The Rust type name of the body (if JSON).
    pub body_type: Option<String>,
    /// Extracted headers.
    pub headers: Vec<ResponseHeader>,
    /// Extracted links.
    pub links: Vec<ParsedLink>,
}

/// Extracts the success response type and its headers/links.
pub fn extract_response_details(
    responses: &Responses,
    components: Option<&ShimComponents>,
) -> AppResult<Option<ParsedResponseDetails>> {
    let mut chosen_response = None;

    // 1. Direct Lookup Priorities
    let priorities = ["200", "201", "2XX", "2xx", "default", "3XX", "3xx"];
    for key in priorities {
        if let Some(r) = responses.responses.get(key) {
            chosen_response = Some(r);
            break;
        }
    }

    // 2. Fallback: Search for any concrete 2xx code
    if chosen_response.is_none() {
        for (key, resp) in &responses.responses {
            if key.starts_with('2') && key.len() == 3 && key.chars().all(char::is_numeric) {
                chosen_response = Some(resp);
                break;
            }
        }
    }

    if let Some(resp_item) = chosen_response {
        let response = match resp_item {
            RefOr::T(val) => Some(val.clone()),
            RefOr::Ref(r) => resolve_response_from_components(r, components),
        };

        if let Some(r) = response {
            // 1. Resolve Body Type
            let body_type = select_response_schema(&r.content)
                .and_then(|media| media.schema.as_ref())
                .map(|schema| map_schema_to_rust_type(schema, true))
                .transpose()?;

            // 2. Resolve Headers
            let mut headers = Vec::new();
            for (name, header_obj) in &r.headers {
                let ty = map_schema_to_rust_type(&header_obj.schema, true)?;
                headers.push(ResponseHeader {
                    name: name.clone(),
                    description: header_obj.description.clone(),
                    ty,
                });
            }

            // 3. Resolve Links
            let mut links = Vec::new();
            for (name, link_val) in &r.links {
                let link_obj = match link_val {
                    RefOr::T(l) => Some(l.clone()),
                    RefOr::Ref(r) => resolve_link_from_ref(r, components),
                };

                if let Some(l) = link_obj {
                    // Map generic Value -> String -> RuntimeExpression
                    let parameters = l
                        .parameters
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                RuntimeExpression::new(v.to_string().trim_matches('"').to_string()),
                            )
                        })
                        .collect();

                    links.push(ParsedLink {
                        name: name.clone(),
                        description: Some(l.description.clone()),
                        operation_id: Some(l.operation_id.clone()),
                        operation_ref: Some(l.operation_ref.clone()),
                        parameters,
                    });
                }
            }

            return Ok(Some(ParsedResponseDetails {
                body_type,
                headers,
                links,
            }));
        }
    }

    Ok(None)
}

/// Selects the most appropriate response content for JSON-like payloads.
///
/// Preference order:
/// 1. `application/json`
/// 2. Any `+json` media type (e.g. `application/vnd.api+json`)
/// 3. `application/*`
/// 4. `*/*`
/// 5. First available entry
fn select_response_schema<'a>(content: &'a IndexMap<String, Content>) -> Option<&'a Content> {
    if let Some(media) = content.get("application/json") {
        return Some(media);
    }

    if let Some((_, media)) = content
        .iter()
        .find(|(k, _)| k.ends_with("+json") || k.as_str() == "application/*+json")
    {
        return Some(media);
    }

    if let Some(media) = content.get("application/*") {
        return Some(media);
    }

    if let Some(media) = content.get("*/*") {
        return Some(media);
    }

    content.iter().next().map(|(_, media)| media)
}

fn resolve_response_from_components(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
) -> Option<utoipa::openapi::Response> {
    let ref_name = r.ref_location.split('/').next_back()?;
    if let Some(comps) = components {
        if let Some(resp_json) = comps.extra.get("responses").and_then(|r| r.get(ref_name)) {
            if let Ok(resp) = serde_json::from_value::<utoipa::openapi::Response>(resp_json.clone())
            {
                return Some(resp);
            }
        }
    }
    None
}

fn resolve_link_from_ref(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
) -> Option<utoipa::openapi::link::Link> {
    let ref_name = r.ref_location.split('/').next_back()?;
    if let Some(comps) = components {
        if let Some(link_json) = comps.extra.get("links").and_then(|l| l.get(ref_name)) {
            if let Ok(link) =
                serde_json::from_value::<utoipa::openapi::link::Link>(link_json.clone())
            {
                return Some(link);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::openapi::{link::LinkBuilder, Content, ResponseBuilder};

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

        let details = extract_response_details(&responses, None).unwrap().unwrap();
        assert_eq!(details.body_type.unwrap(), "User");
    }

    #[test]
    fn test_extract_response_links_with_runtime_expression() {
        let link = LinkBuilder::new()
            .operation_id("getUser")
            .description("Get user")
            .parameter("userId", "$request.path.id")
            .build();

        let response = ResponseBuilder::new()
            .description("Linked Response")
            .link("UserLink", link)
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&responses, None).unwrap().unwrap();
        let l = &details.links[0];
        let expr = l.parameters.get("userId").unwrap();

        assert_eq!(expr.as_str(), "$request.path.id");
        assert!(expr.is_expression());
    }

    #[test]
    fn test_extract_vendor_json_media_type() {
        let response = ResponseBuilder::new()
            .description("Vendor JSON")
            .content(
                "application/vnd.api+json",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/User",
                )))),
            )
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&responses, None).unwrap().unwrap();
        assert_eq!(details.body_type.unwrap(), "User");
    }

    #[test]
    fn test_extract_wildcard_media_type() {
        let response = ResponseBuilder::new()
            .description("Wildcard")
            .content(
                "*/*",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/Anything",
                )))),
            )
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&responses, None).unwrap().unwrap();
        assert_eq!(details.body_type.unwrap(), "Anything");
    }
}
