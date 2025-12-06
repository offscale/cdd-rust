#![deny(missing_docs)]

//! # Route Shims
//!
//! Generic structures acting as an Intermediate Deserialization Layer.
//! These structs map directly to OpenAPI YAML objects but use generic `serde_json::Value`
//! or our `ShimParameter` for robustness before being converted to strict `models`.

use crate::oas::resolver::ShimParameter;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use utoipa::openapi::request_body::RequestBody;
use utoipa::openapi::{RefOr, Responses};

/// Schema for the root document (Paths and Webhooks).
#[derive(Deserialize)]
pub struct ShimOpenApi {
    /// Components section used for reference resolution.
    #[serde(default)]
    pub components: Option<Value>,
    /// Path items.
    #[serde(default)]
    pub paths: BTreeMap<String, ShimPathItem>,
    /// Webhook items.
    #[serde(default)]
    pub webhooks: Option<BTreeMap<String, RefOr<ShimPathItem>>>,
}

/// A Path Item containing operations for a specific URL/Webhook.
#[derive(Deserialize, Clone)]
pub struct ShimPathItem {
    /// Parameters common to all operations in this path.
    #[serde(default)]
    pub parameters: Option<Vec<RefOr<ShimParameter>>>,
    /// GET operation.
    pub get: Option<ShimOperation>,
    /// POST operation.
    pub post: Option<ShimOperation>,
    /// PUT operation.
    pub put: Option<ShimOperation>,
    /// DELETE operation.
    pub delete: Option<ShimOperation>,
    /// PATCH operation.
    pub patch: Option<ShimOperation>,
    /// OPTIONS operation.
    pub options: Option<ShimOperation>,
    /// HEAD operation.
    pub head: Option<ShimOperation>,
    /// TRACE operation.
    pub trace: Option<ShimOperation>,
}

/// A single HTTP Operation definition.
#[derive(Deserialize, Clone)]
pub struct ShimOperation {
    /// Unique identifier for the operation.
    #[serde(rename = "operationId")]
    pub operation_id: Option<String>,
    /// Operation-specific parameters.
    #[serde(default)]
    pub parameters: Option<Vec<RefOr<ShimParameter>>>,
    /// Request Body.
    #[serde(rename = "requestBody")]
    pub request_body: Option<RefOr<RequestBody>>,
    /// Responses.
    #[serde(default)]
    pub responses: Responses,
    /// Security requirements (raw JSON values to be generic).
    #[serde(default)]
    pub security: Option<Vec<Value>>,
    /// Callbacks definition.
    #[serde(default)]
    pub callbacks: Option<BTreeMap<String, RefOr<BTreeMap<String, ShimPathItem>>>>,
}
