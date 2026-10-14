#![deny(missing_docs)]

//! HTTP route handlers grouped by OpenAPI tag (`pet`, `store`, `user`).

/// Pet management route handlers.
pub mod pet;
/// Store inventory and order route handlers.
pub mod store;
/// User management and authentication route handlers.
pub mod user;
