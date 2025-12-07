#![deny(missing_docs)]

//! # Response Resolution
//!
//! Logic for resolving OpenAPI Responses into Rust types.
//!
//! Handles:
//! - Extracting Response Body types (JSON)
//! - Extracting Response Headers (OAS 3.2 support)
//! - Extracting Response Links (HATEOAS support)
//! - Wildcard response support (2XX, 3XX, default)

use crate::error::AppResult;
use crate::oas::models::{ParsedLink, ResponseHeader};
use crate::oas::resolver::types::map_schema_to_rust_type;
use crate::oas::routes::shims::ShimComponents;
use utoipa::openapi::{RefOr, Responses};

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
///
/// Priority order:
/// 1. Explicit success: "200", "201"
/// 2. Wildcard success: "2XX" (or "2xx")
/// 3. Default: "default"
/// 4. Redirects (if no success defined): "3XX" (or "3xx")
/// 5. Fallback: First key starting with "2" (e.g. "202")
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

    // 2. Fallback: Search for any concrete 2xx code if no high-level match
    if chosen_response.is_none() {
        // BTree iteration is sorted, so we get 202 before 204 etc.
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
            let body_type = if let Some(media) = r.content.get("application/json") {
                if let Some(schema) = &media.schema {
                    Some(map_schema_to_rust_type(schema, true)?)
                } else {
                    None
                }
            } else {
                None
            };

            // 2. Resolve Headers
            let mut headers = Vec::new();
            for (name, header_obj) in &r.headers {
                // Determine Rust type for the header value
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
                    // Extract parameters (HashMap<String, Value>) -> HashMap<String, String>
                    // We map Any Value to String to simplify the IR.
                    // Runtime expressions are strings, constants might be numbers/bools.
                    let parameters = l
                        .parameters
                        .iter()
                        .map(|(k, v)| (k.clone(), v.to_string().trim_matches('"').to_string()))
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
    use utoipa::openapi::{link::LinkBuilder, Content, HeaderBuilder, ResponseBuilder};

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
        assert!(details.headers.is_empty());
        assert!(details.links.is_empty());
    }

    #[test]
    fn test_extract_response_with_headers_and_links_and_params() {
        let header = HeaderBuilder::new()
            .description(Some("Rate limit"))
            .schema(RefOr::Ref(utoipa::openapi::Ref::new(
                "#/components/schemas/Integer",
            )))
            .build();

        let link = LinkBuilder::new()
            .operation_id("getUser")
            .description("Get user details")
            .parameter("userId", "$request.path.id")
            .build();

        let response = ResponseBuilder::new()
            .description("Headers Test")
            .header("X-Rate-Limit", header)
            .link("UserLink", link)
            .build();

        let mut responses = Responses::new();
        responses.responses.insert("200".into(), RefOr::T(response));

        let details = extract_response_details(&responses, None).unwrap().unwrap();

        assert_eq!(details.headers.len(), 1);
        assert_eq!(details.headers[0].name, "X-Rate-Limit");

        assert_eq!(details.links.len(), 1);
        let l = &details.links[0];
        assert_eq!(l.name, "UserLink");
        assert_eq!(l.operation_id.as_deref(), Some("getUser"));
        assert_eq!(
            l.parameters.get("userId").map(|s| s.as_str()),
            Some("$request.path.id")
        );
    }

    #[test]
    fn test_extract_wildcard_2xx_response() {
        let response = ResponseBuilder::new()
            .description("Generic Success")
            .content(
                "application/json",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/Success",
                )))),
            )
            .build();

        let mut responses = Responses::new();
        // Insert wildcards
        responses.responses.insert("2XX".into(), RefOr::T(response));
        responses.responses.insert(
            "default".into(),
            RefOr::T(ResponseBuilder::new().description("Error").build()),
        );

        let details = extract_response_details(&responses, None)
            .unwrap()
            .expect("Should match 2XX");

        assert_eq!(details.body_type.as_deref(), Some("Success"));
    }

    #[test]
    fn test_extract_default_response_fallback() {
        let response = ResponseBuilder::new()
            .description("Default Fallback")
            .content(
                "application/json",
                Content::new(Some(RefOr::Ref(utoipa::openapi::Ref::new(
                    "#/components/schemas/Fallback",
                )))),
            )
            .build();

        let mut responses = Responses::new();
        responses
            .responses
            .insert("default".into(), RefOr::T(response));

        let details = extract_response_details(&responses, None)
            .unwrap()
            .expect("Should match default");

        assert_eq!(details.body_type.as_deref(), Some("Fallback"));
    }
}
