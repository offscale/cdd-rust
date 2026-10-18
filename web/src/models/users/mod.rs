#![deny(missing_docs)]

//! User table models.

/// Generated user models and Diesel mappings.
pub mod generated;
/// Re-export generated models for convenient access.
pub use generated::*;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use diesel::associations::Identifiable;
    use diesel::SelectableHelper;
    use uuid::Uuid;

    #[test]
    fn test_users_struct_integrity() {
        let now = Utc::now().naive_utc();
        let id = Uuid::new_v4();
        let _ = <crate::schema::users::table as diesel::Table>::all_columns();
        let _ = <Users as SelectableHelper<diesel::pg::Pg>>::as_select();
        let user = Users {
            id,
            email: "test@example.com".to_string(),
            password_hash: "secret".to_string(),
            created_at: now,
            updated_at: now,
        };
        let cloned = user.clone();
        assert_eq!(*cloned.id(), id);
        assert_eq!((&user).id(), &id);
        assert_eq!(user.email, "test@example.com");

        let json_user = serde_json::to_string(&user).expect("serialize user");
        let de_user: Users = serde_json::from_str(&json_user).expect("deserialize user");
        assert_eq!(de_user.id, id);

        let create = CreateUsers {
            id,
            email: "test@example.com".to_string(),
            password_hash: "secret".to_string(),
            created_at: now,
            updated_at: now,
        };
        let json_create = serde_json::to_string(&create.clone()).expect("serialize create");
        let de_create: CreateUsers =
            serde_json::from_str(&json_create).expect("deserialize create");
        assert_eq!(de_create.id, id);

        let update = UpdateUsers {
            email: Some("new@example.com".to_string()),
            password_hash: Some("hash".to_string()),
            created_at: Some(now),
            updated_at: Some(now),
        };
        let json_update = serde_json::to_string(&update.clone()).expect("serialize update");
        let de_update: UpdateUsers =
            serde_json::from_str(&json_update).expect("deserialize update");
        assert_eq!(de_update, update);
        assert!(!format!("{:?}{:?}{:?}", user, create, update).is_empty());
    }
}
