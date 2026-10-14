#![deny(missing_docs)]

//! Data Access Objects (DAOs) for the CDD Server.
//!
//! This module isolates data access logic, providing abstract traits and multiple
//! implementations (Stub, Concrete) to support orthogonal server states.

/// Database connection configuration and R2D2 pool setup.
pub mod connection;
/// DAO factory and dependency injection container.
pub mod factory;
/// User repository trait and implementations.
pub mod users;
