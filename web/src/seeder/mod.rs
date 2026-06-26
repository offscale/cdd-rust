//! Fake Data Seeder & Dependency Graph Generator.
//!
//! This module is responsible for generating localized, fake data for the CDD Server.
//! It manages referential integrity by maintaining an `EntityPool` of generated primary keys.
//! When generating dependent records (e.g., Posts or Comments), the factories randomly
//! select foreign keys from the `EntityPool`'s cached parent IDs, ensuring valid relationships.

use crate::dao::users::UserDao;
use crate::error::ServerError;
use crate::models::users::CreateUsers;
use chrono::Utc;
use fake::faker::internet::en::Password;
use fake::faker::internet::en::SafeEmail;
use fake::Fake;
use std::sync::Arc;
use uuid::Uuid;

/// Entity Pool to cache IDs of successfully generated records.
/// Used to maintain referential integrity when generating dependent data.
#[derive(Default, Debug, Clone)]
pub struct EntityPool {
    /// Cached User IDs.
    pub users: Vec<Uuid>,
}

impl EntityPool {
    /// Creates a new, empty EntityPool.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Factory function to generate a fake User creation struct.
///
/// Returns a fully populated `CreateUsers` instance with realistic fake data.
pub fn fake_user_factory() -> CreateUsers {
    let now = Utc::now().naive_utc();
    CreateUsers {
        id: Uuid::new_v4(),
        email: SafeEmail().fake(),
        password_hash: Password(8..15).fake(),
        created_at: now,
        updated_at: now,
    }
}

/// Batch insert function that orchestrates the seeder.
///
/// Topologically generates records (e.g. Users) and caches their IDs in the EntityPool.
/// Resolves dependencies (if any) before generating children.
pub async fn seed_database(user_dao: Arc<dyn UserDao>) -> Result<EntityPool, ServerError> {
    let mut pool = EntityPool::new();

    // 1. Generate Users
    let user_count = 10;
    for _ in 0..user_count {
        let fake_user = fake_user_factory();
        let inserted = user_dao.create_user(fake_user).await?;
        pool.users.push(inserted.id);
    }

    // 2. Further relational generation goes here (e.g., Posts using `pool.users`),
    // ensuring referential integrity without FK violations.

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dao::users::StubUserDao;

    #[actix_web::test]
    async fn test_fake_user_factory() {
        let user = fake_user_factory();
        assert!(!user.email.is_empty());
        assert!(!user.password_hash.is_empty());
    }

    #[actix_web::test]
    async fn test_seed_database_stub() {
        let dao = Arc::new(StubUserDao);
        let res = seed_database(dao).await;
        // Stub DAO returns NotImplemented, so seed_database should fail with it.
        assert!(matches!(res, Err(ServerError::NotImplemented)));
    }
}

#[cfg(test)]
mod additional_seeder_tests {
    use super::*;
    use crate::dao::users::UserDao;
    use crate::models::users::{CreateUsers, UpdateUsers, Users};
    use async_trait::async_trait;

    struct SuccessUserDao;
    #[async_trait]
    impl UserDao for SuccessUserDao {
        async fn get_user(&self, _id: Uuid) -> Result<Option<Users>, ServerError> {
            Ok(None)
        }
        async fn get_user_by_name(&self, _username: &str) -> Result<Option<Users>, ServerError> {
            Ok(None)
        }
        async fn create_user(&self, user: CreateUsers) -> Result<Users, ServerError> {
            Ok(Users {
                id: user.id,
                email: user.email,
                password_hash: "hash".to_string(),
                created_at: user.created_at,
                updated_at: user.updated_at,
            })
        }
        async fn update_user(&self, _id: Uuid, _user: UpdateUsers) -> Result<Users, ServerError> {
            Err(ServerError::NotImplemented)
        }
        async fn delete_user(&self, _id: Uuid) -> Result<(), ServerError> {
            Err(ServerError::NotImplemented)
        }
    }

    #[actix_web::test]
    async fn test_seed_database_success() {
        let dao = Arc::new(SuccessUserDao);
        let res = seed_database(dao).await.expect("must succeed");
        assert_eq!(res.users.len(), 10);
    }
}
