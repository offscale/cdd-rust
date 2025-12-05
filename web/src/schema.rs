// @generated automatically by Diesel CLI.
// Manual doc comments added for compliance.

//! Database Schema.

diesel::table! {
    /// The users table.
    users (id) {
        /// Primary Key (UUID).
        id -> Uuid,
        /// Email address.
        email -> Varchar,
        /// Encrypted password.
        password_hash -> Varchar,
        /// Creation timestamp.
        created_at -> Timestamp,
        /// Update timestamp.
        updated_at -> Timestamp,
    }
}
