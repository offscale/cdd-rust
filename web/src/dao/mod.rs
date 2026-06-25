//! Data Access Objects (DAOs) for the CDD Server.
//!
//! This module isolates data access logic, providing abstract traits and multiple
//! implementations (Stub, Concrete) to support orthogonal server states.

pub mod connection;
pub mod factory;
pub mod users;
