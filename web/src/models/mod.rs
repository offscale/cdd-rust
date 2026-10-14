#![deny(missing_docs)]

//! Domain and API data transfer models for the CDD Web Server.

/// Dynamic JSON representation of a Pet entity.
pub type Pet = serde_json::Value;
/// Dynamic JSON representation of a Store Order entity.
pub type Order = serde_json::Value;
/// Dynamic JSON representation of a User entity.
pub type User = serde_json::Value;
/// Dynamic JSON representation of a standard API response envelope.
pub type ApiResponse = serde_json::Value;

/// Generated route parameter and query models for HTTP handlers.
pub mod handlers;
/// Database-backed User table models.
pub mod users;
