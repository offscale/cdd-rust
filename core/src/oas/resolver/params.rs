#![deny(missing_docs)]

//! # Parameter Resolution
//!
//! Logic for resolving OpenAPI Parameters into internal `RouteParam` structs.
//! Handles styles, explode, and legacy Swagger 2.0 compatibility.

use crate::error::{AppError, AppResult};
use crate::oas::models::{ParamSource, ParamStyle, RouteParam};
use crate::oas::resolver::types::map_schema_to_rust_type;
use crate::oas::routes::shims::ShimComponents;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use utoipa::openapi::content::Content;
use utoipa::openapi::{schema::Schema, RefOr};

/// A local shim for Parameter to ensure robust parsing of fields.
/// Includes OAS 3.x style fields and OAS 2.0 `collectionFormat`.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct ShimParameter {
    /// Name of the parameter.
    pub name: String,
    /// Location of the parameter (query, path, header, cookie, querystring).
    #[serde(rename = "in")]
    pub parameter_in: String,
    /// Whether the parameter is required.
    #[serde(default)]
    pub required: bool,
    /// Schema definition.
    pub schema: Option<RefOr<Schema>>,
    /// Content map (OAS 3.x complex parameter serialization).
    /// Mutually exclusive with `schema`.
    pub content: Option<BTreeMap<String, Content>>,
    /// Serialization style (OAS 3.x).
    pub style: Option<String>,
    /// Explode modifier (OAS 3.x).
    pub explode: Option<bool>,
    /// Allow reserved characters (OAS 3.x).
    #[serde(rename = "allowReserved", default)]
    pub allow_reserved: bool,
    /// Collection format (OAS 2.0 compatibility).
    #[serde(rename = "collectionFormat")]
    pub collection_format: Option<String>,
}

// Manual Debug implementation to avoid strict type bounds on fields derived by macros sometimes.
impl fmt::Debug for ShimParameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShimParameter")
            .field("name", &self.name)
            .field("parameter_in", &self.parameter_in)
            .field("required", &self.required)
            .field("style", &self.style)
            .field("explode", &self.explode)
            .field("allow_reserved", &self.allow_reserved)
            .field("schema", &self.schema.as_ref().map(|_| "Some(Schema)"))
            .field("content", &self.content.as_ref().map(|_| "Some(Content)"))
            .finish()
    }
}

/// Resolves a list of OpenAPI parameters into internal `RouteParam` structs.
pub fn resolve_parameters(
    params: &[RefOr<ShimParameter>],
    components: Option<&ShimComponents>,
) -> AppResult<Vec<RouteParam>> {
    let mut result = Vec::new();
    for param_or_ref in params {
        let param_opt = match param_or_ref {
            RefOr::T(param) => Some(param.clone()),
            RefOr::Ref(r) => resolve_parameter_ref(r, components),
        };

        if let Some(param) = param_opt {
            let route_param = process_parameter(&param)?;
            result.push(route_param);
        }
    }
    Ok(result)
}

/// Helper to resolve a `Ref` to its target Parameter definition.
fn resolve_parameter_ref(
    r: &utoipa::openapi::Ref,
    components: Option<&ShimComponents>,
) -> Option<ShimParameter> {
    let ref_name = r.ref_location.split('/').next_back()?;

    if let Some(comps) = components {
        // Note: Generic components are now in `extra`.
        if let Some(param_json) = comps.extra.get("parameters").and_then(|p| p.get(ref_name)) {
            if let Ok(param) = serde_json::from_value::<ShimParameter>(param_json.clone()) {
                return Some(param);
            }
        }
    }
    None
}

/// Helper to convert a resolved ShimParameter to our internal RouteParam.
/// Implements default resolution logic for style/explode based on the OAS 3.2.0 spec.
fn process_parameter(param: &ShimParameter) -> AppResult<RouteParam> {
    let name = param.name.clone();
    let source = match param.parameter_in.as_str() {
        "path" => ParamSource::Path,
        "query" => ParamSource::Query,
        // OAS 3.2.0 Support: entire query string
        "querystring" => ParamSource::QueryString,
        "header" => ParamSource::Header,
        "cookie" => ParamSource::Cookie,
        _ => ParamSource::Query,
    };

    // Determine style based on OAS3 fields, defaulting to OAS2 logic if missing
    let style = resolve_style(
        param.style.as_deref(),
        param.collection_format.as_deref(),
        &source,
    );

    // Determine explode. Defaults depend on style.
    let explode = resolve_explode(param.explode, &style);

    // Resolve type from `schema` OR `content`
    let ty = if let Some(schema_ref) = &param.schema {
        map_schema_to_rust_type(schema_ref, param.required)?
    } else if let Some(content) = &param.content {
        // OAS 3.0 spec: "The map MUST only contain one entry."
        // We take the first one found.
        if let Some((_media_type, media_obj)) = content.iter().next() {
            if let Some(s) = &media_obj.schema {
                map_schema_to_rust_type(s, param.required)?
            } else {
                "serde_json::Value".to_string()
            }
        } else {
            return Err(AppError::General(format!(
                "Parameter '{}' has empty content map",
                name
            )));
        }
    } else {
        "String".to_string()
    };

    Ok(RouteParam {
        name,
        source,
        ty,
        style,
        explode,
        allow_reserved: param.allow_reserved,
    })
}

/// Resolves the parameter style.
///
/// Priorities:
/// 1. Explicit `style` (OAS 3).
/// 2. Mapped `collectionFormat` (OAS 2).
/// 3. Default based on `in` location (OAS 3 spec).
fn resolve_style(
    style_str: Option<&str>,
    collection_format: Option<&str>,
    source: &ParamSource,
) -> Option<ParamStyle> {
    if let Some(s) = style_str {
        return match s {
            "matrix" => Some(ParamStyle::Matrix),
            "label" => Some(ParamStyle::Label),
            "form" => Some(ParamStyle::Form),
            "simple" => Some(ParamStyle::Simple),
            "spaceDelimited" => Some(ParamStyle::SpaceDelimited),
            "pipeDelimited" => Some(ParamStyle::PipeDelimited),
            "deepObject" => Some(ParamStyle::DeepObject),
            _ => None,
        };
    }

    if let Some(cf) = collection_format {
        // Map OAS 2.0 collectionFormat to style
        return match cf {
            "csv" => match source {
                ParamSource::Query | ParamSource::Cookie | ParamSource::QueryString => {
                    Some(ParamStyle::Form)
                }
                ParamSource::Path | ParamSource::Header => Some(ParamStyle::Simple),
            },
            "ssv" => Some(ParamStyle::SpaceDelimited),
            "tsv" => Some(ParamStyle::SpaceDelimited), // Approximate mapping
            "pipes" => Some(ParamStyle::PipeDelimited),
            "multi" => Some(ParamStyle::Form), // 'multi' implies form with explode=true
            _ => None,
        };
    }

    // OAS 3.2.0 Defaults
    match source {
        // `query` and `cookie` (and implicit `querystring`) default to Form
        ParamSource::Query | ParamSource::QueryString => Some(ParamStyle::Form),
        ParamSource::Path => Some(ParamStyle::Simple),
        ParamSource::Header => Some(ParamStyle::Simple),
        ParamSource::Cookie => Some(ParamStyle::Form),
    }
}

/// Resolves the explode property.
///
/// Defaults:
/// - `style: form` -> true
/// - `style: cookie` -> true (OAS 3.2 spec note: "When style is form or cookie, the default value is true")
/// - Others -> false
fn resolve_explode(explicit: Option<bool>, style: &Option<ParamStyle>) -> bool {
    if let Some(e) = explicit {
        return e;
    }

    match style {
        Some(ParamStyle::Form) => true,
        // ParamStyle doesn't have explicit Cookie variant, Cookie uses Form style usually.
        // If user explicitly set style="form" (default for cookie), it returns true.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oas::models::{ParamSource, ParamStyle};
    use utoipa::openapi::schema::{Schema, Type};
    use utoipa::openapi::{ContentBuilder, ObjectBuilder};

    #[test]
    fn test_resolve_parameters_defaults_query() {
        // Case: Query param, no style/explode specified.
        // Expect: Style=Form, Explode=True.
        let param = ShimParameter {
            name: "q".to_string(),
            parameter_in: "query".to_string(),
            required: false,
            schema: None,
            content: None,
            style: None,
            explode: None,
            allow_reserved: false,
            collection_format: None,
        };

        let processed = process_parameter(&param).unwrap();
        assert_eq!(processed.source, ParamSource::Query);
        assert_eq!(processed.style, Some(ParamStyle::Form));
        assert_eq!(processed.explode, true); // Form defaults to true
    }

    #[test]
    fn test_resolve_parameters_complex_content() {
        // Case: Query param defined with `content` instead of `schema`
        // e.g. ?filter={"foo":"bar"} (application/json)
        let schema = ObjectBuilder::new().schema_type(Type::Object).build();
        let content_item = ContentBuilder::new()
            .schema(Some(RefOr::T(Schema::Object(schema))))
            .build();

        let mut content_map = BTreeMap::new();
        content_map.insert("application/json".into(), content_item);

        let param = ShimParameter {
            name: "filter".to_string(),
            parameter_in: "query".to_string(),
            required: true,
            schema: None,
            content: Some(content_map),
            style: None,
            explode: None,
            allow_reserved: false,
            collection_format: None,
        };

        let processed = process_parameter(&param).unwrap();
        // Since schema is Object, map_schema_to_rust_type returns "serde_json::Value" unless typed struct ref
        assert_eq!(processed.ty, "serde_json::Value");
        assert_eq!(processed.source, ParamSource::Query);
    }

    #[test]
    fn test_resolve_parameters_querystring_oas_3_2() {
        // Case: OAS 3.2 in: querystring
        // Expect: Source=QueryString, Style=Form (Default)
        let param = ShimParameter {
            name: "filter".to_string(),
            parameter_in: "querystring".to_string(),
            required: true,
            schema: None,
            content: None,
            style: None,
            explode: None,
            allow_reserved: false,
            collection_format: None,
        };

        let processed = process_parameter(&param).unwrap();
        assert_eq!(processed.source, ParamSource::QueryString);
        assert_eq!(processed.style, Some(ParamStyle::Form));
    }

    #[test]
    fn test_resolve_parameters_defaults_path() {
        // Case: Path param.
        // Expect: Style=Simple, Explode=False.
        let param = ShimParameter {
            name: "id".to_string(),
            parameter_in: "path".to_string(),
            required: true,
            schema: None,
            content: None,
            style: None,
            explode: None,
            allow_reserved: false,
            collection_format: None,
        };

        let processed = process_parameter(&param).unwrap();
        assert_eq!(processed.source, ParamSource::Path);
        assert_eq!(processed.style, Some(ParamStyle::Simple));
        assert_eq!(processed.explode, false); // Simple defaults to false
    }

    #[test]
    fn test_resolve_swagger2_collection_format() {
        // Case: Query param with collectionFormat="ssv"
        // Expect: Style=SpaceDelimited
        let param = ShimParameter {
            name: "tags".to_string(),
            parameter_in: "query".to_string(),
            required: false,
            schema: None,
            content: None,
            style: None,
            explode: None,
            allow_reserved: false,
            collection_format: Some("ssv".to_string()),
        };

        let processed = process_parameter(&param).unwrap();
        assert_eq!(processed.style, Some(ParamStyle::SpaceDelimited));
    }

    #[test]
    fn test_resolve_explicit_overrides() {
        // Case: Header param with explicit explode=true
        let param = ShimParameter {
            name: "X-Ids".to_string(),
            parameter_in: "header".to_string(),
            required: false,
            schema: None,
            content: None,
            style: None, // defaults to simple
            explode: Some(true),
            allow_reserved: false,
            collection_format: None,
        };

        let processed = process_parameter(&param).unwrap();
        assert_eq!(processed.style, Some(ParamStyle::Simple));
        assert_eq!(processed.explode, true);
    }

    #[test]
    fn test_resolve_reusable_parameters() {
        // New structure requires generic components to be in 'extra' for legacy resolution.
        // ShimComponents handles this via flattening.
        let components_json = serde_json::json!({
            "parameters": {
                "limitParam": {
                    "name": "limit",
                    "in": "query",
                    "style": "form",
                    "explode": false,
                    "schema": { "type": "integer" }
                }
            }
        });

        let components: ShimComponents = serde_json::from_value(components_json).unwrap();

        let op_params = vec![RefOr::Ref(utoipa::openapi::Ref::new(
            "#/components/parameters/limitParam",
        ))];

        let resolved = resolve_parameters(&op_params, Some(&components)).unwrap();

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "limit");
        assert_eq!(resolved[0].style, Some(ParamStyle::Form));
        assert_eq!(resolved[0].explode, false);
    }
}
