#![deny(missing_docs)]

//! User DAO implementations (`UserDao`, `StubUserDao`, `ConcreteUserDao`).

use crate::error::ServerError;
use crate::models::users::{CreateUsers, UpdateUsers, Users};
use async_trait::async_trait;
use diesel::prelude::*;
use uuid::Uuid;

#[cfg(not(target_os = "wasi"))]
use diesel::r2d2::{ConnectionManager, Pool};

/// Abstract interface for User data access operations.
#[async_trait]
pub trait UserDao: Send + Sync {
    /// Retrieves a user by primary key UUID.
    ///
    /// # Arguments
    /// * `id` - User UUID.
    ///
    /// # Returns
    /// * `Option<Users>` record if found.
    ///
    /// # Errors
    /// * Returns [`ServerError`] on database failure or if not implemented.
    async fn get_user(&self, id: Uuid) -> Result<Option<Users>, ServerError>;

    /// Retrieves a user by email or username.
    ///
    /// # Arguments
    /// * `username` - User email/username string.
    ///
    /// # Returns
    /// * `Option<Users>` record if found.
    ///
    /// # Errors
    /// * Returns [`ServerError`] on database failure or if not implemented.
    async fn get_user_by_name(&self, username: &str) -> Result<Option<Users>, ServerError>;

    /// Inserts a new user into the database.
    ///
    /// # Arguments
    /// * `user` - [`CreateUsers`] insertion payload.
    ///
    /// # Returns
    /// * Newly created [`Users`] record.
    ///
    /// # Errors
    /// * Returns [`ServerError`] on constraint violation, connection failure, or stub mode.
    async fn create_user(&self, user: CreateUsers) -> Result<Users, ServerError>;

    /// Updates an existing user record by UUID.
    ///
    /// # Arguments
    /// * `id` - User UUID.
    /// * `user` - [`UpdateUsers`] changeset payload.
    ///
    /// # Returns
    /// * Updated [`Users`] record.
    ///
    /// # Errors
    /// * Returns [`ServerError`] on database failure or if not implemented.
    async fn update_user(&self, id: Uuid, user: UpdateUsers) -> Result<Users, ServerError>;

    /// Deletes a user record by UUID.
    ///
    /// # Arguments
    /// * `id` - User UUID.
    ///
    /// # Returns
    /// * `()` on success.
    ///
    /// # Errors
    /// * Returns [`ServerError`] on database failure or if not implemented.
    async fn delete_user(&self, id: Uuid) -> Result<(), ServerError>;
}

/// Stub implementation of [`UserDao`] returning [`ServerError::NotImplemented`].
#[derive(Default)]
pub struct StubUserDao;

#[async_trait]
impl UserDao for StubUserDao {
    async fn get_user(&self, _id: Uuid) -> Result<Option<Users>, ServerError> {
        Err(ServerError::NotImplemented)
    }

    async fn get_user_by_name(&self, _username: &str) -> Result<Option<Users>, ServerError> {
        Err(ServerError::NotImplemented)
    }

    async fn create_user(&self, _user: CreateUsers) -> Result<Users, ServerError> {
        Err(ServerError::NotImplemented)
    }

    async fn update_user(&self, _id: Uuid, _user: UpdateUsers) -> Result<Users, ServerError> {
        Err(ServerError::NotImplemented)
    }

    async fn delete_user(&self, _id: Uuid) -> Result<(), ServerError> {
        Err(ServerError::NotImplemented)
    }
}

/// Concrete implementation of [`UserDao`] using a Diesel PostgreSQL R2D2 connection pool.
#[cfg(not(target_os = "wasi"))]
pub struct ConcreteUserDao {
    /// R2D2 PostgreSQL connection pool.
    pool: Pool<ConnectionManager<PgConnection>>,
}

#[cfg(not(target_os = "wasi"))]
impl ConcreteUserDao {
    /// Creates a new [`ConcreteUserDao`] backed by the given connection pool.
    ///
    /// # Arguments
    /// * `pool` - R2D2 PostgreSQL connection pool.
    ///
    /// # Returns
    /// * Initialized [`ConcreteUserDao`].
    pub fn new(pool: Pool<ConnectionManager<PgConnection>>) -> Self {
        Self { pool }
    }
}

#[cfg(not(target_os = "wasi"))]
#[async_trait]
impl UserDao for ConcreteUserDao {
    async fn get_user(&self, target_id: Uuid) -> Result<Option<Users>, ServerError> {
        use crate::schema::users::dsl::*;
        let mut conn = self
            .pool
            .get()
            .map_err(|e| ServerError::SyncError(e.to_string()))?;

        let user = users.find(target_id).first::<Users>(&mut conn).optional()?;
        Ok(user)
    }

    async fn get_user_by_name(&self, username: &str) -> Result<Option<Users>, ServerError> {
        use crate::schema::users::dsl::*;
        let mut conn = self
            .pool
            .get()
            .map_err(|e| ServerError::SyncError(e.to_string()))?;

        let user = users
            .filter(email.eq(username))
            .first::<Users>(&mut conn)
            .optional()?;
        Ok(user)
    }

    async fn create_user(&self, user: CreateUsers) -> Result<Users, ServerError> {
        use crate::schema::users::dsl::*;
        let mut conn = self
            .pool
            .get()
            .map_err(|e| ServerError::SyncError(e.to_string()))?;

        let inserted = diesel::insert_into(users)
            .values(&user)
            .get_result::<Users>(&mut conn)?;
        Ok(inserted)
    }

    async fn update_user(&self, target_id: Uuid, user: UpdateUsers) -> Result<Users, ServerError> {
        use crate::schema::users::dsl::*;
        let mut conn = self
            .pool
            .get()
            .map_err(|e| ServerError::SyncError(e.to_string()))?;

        let updated = diesel::update(users.find(target_id))
            .set(&user)
            .get_result::<Users>(&mut conn)?;
        Ok(updated)
    }

    async fn delete_user(&self, target_id: Uuid) -> Result<(), ServerError> {
        use crate::schema::users::dsl::*;
        let mut conn = self
            .pool
            .get()
            .map_err(|e| ServerError::SyncError(e.to_string()))?;

        diesel::delete(users.find(target_id)).execute(&mut conn)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix_web::test]
    async fn test_stub_user_dao() {
        let dao = StubUserDao;
        let id = Uuid::new_v4();
        let now = chrono::Utc::now().naive_utc();
        assert!(matches!(
            dao.get_user(id).await,
            Err(ServerError::NotImplemented)
        ));
        assert!(matches!(
            dao.get_user_by_name("alice").await,
            Err(ServerError::NotImplemented)
        ));
        assert!(matches!(
            dao.create_user(CreateUsers {
                id,
                email: "a@b.c".to_string(),
                password_hash: "h".to_string(),
                created_at: now,
                updated_at: now,
            })
            .await,
            Err(ServerError::NotImplemented)
        ));
        assert!(matches!(
            dao.update_user(
                id,
                UpdateUsers {
                    email: None,
                    password_hash: None,
                    created_at: None,
                    updated_at: None,
                }
            )
            .await,
            Err(ServerError::NotImplemented)
        ));
        assert!(matches!(
            dao.delete_user(id).await,
            Err(ServerError::NotImplemented)
        ));
    }

    #[actix_web::test]
    async fn test_concrete_user_dao_pool_errors() {
        let manager = ConnectionManager::<PgConnection>::new("postgres://127.0.0.1:1/cdd");
        let pool = Pool::builder()
            .max_size(1)
            .connection_timeout(std::time::Duration::from_millis(20))
            .build_unchecked(manager);
        let dao = ConcreteUserDao::new(pool);
        let id = Uuid::new_v4();
        let now = chrono::Utc::now().naive_utc();

        assert!(dao.get_user(id).await.is_err());
        assert!(dao.get_user_by_name("alice").await.is_err());
        assert!(dao
            .create_user(CreateUsers {
                id,
                email: "a@b.c".to_string(),
                password_hash: "h".to_string(),
                created_at: now,
                updated_at: now,
            })
            .await
            .is_err());
        assert!(dao
            .update_user(
                id,
                UpdateUsers {
                    email: None,
                    password_hash: None,
                    created_at: None,
                    updated_at: None,
                }
            )
            .await
            .is_err());
        assert!(dao.delete_user(id).await.is_err());
    }
}
