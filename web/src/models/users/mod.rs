//! User table models.

/// Generated user models and Diesel mappings.
pub mod generated;
/// Re-export generated models for convenient access.
pub use generated::*;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_users_struct_integrity() {
        let now = Utc::now().naive_utc();
        let user = Users {
            id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            password_hash: "secret".to_string(),
            created_at: now,
            updated_at: now,
        };
        let _ = user.email;
    }
}
