//! User DAO implementations.

use crate::error::ServerError;
use crate::models::users::{CreateUsers, UpdateUsers, Users};
use async_trait::async_trait;
use diesel::prelude::*;
use uuid::Uuid;

#[cfg(not(target_os = "wasi"))]
use diesel::r2d2::{ConnectionManager, Pool};

/// Abstract interface for User Data Access Object.
#[async_trait]
pub trait UserDao: Send + Sync {
    /// Retrieve a user by UUID.
    async fn get_user(&self, id: Uuid) -> Result<Option<Users>, ServerError>;

    /// Retrieve a user by email/username.
    async fn get_user_by_name(&self, username: &str) -> Result<Option<Users>, ServerError>;

    /// Create a new user.
    async fn create_user(&self, user: CreateUsers) -> Result<Users, ServerError>;

    /// Update an existing user.
    async fn update_user(&self, id: Uuid, user: UpdateUsers) -> Result<Users, ServerError>;

    /// Delete a user.
    async fn delete_user(&self, id: Uuid) -> Result<(), ServerError>;
}

/// Stub implementation of UserDao returning `NotImplementedError` or defaults.
#[derive(Default)]
pub struct StubUserDao;

#[async_trait]
#[cfg(not(tarpaulin_include))]
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

/// Concrete, DB-backed implementation of UserDao.
#[cfg(not(target_os = "wasi"))]
#[cfg(not(tarpaulin_include))]
pub struct ConcreteUserDao {
    /// The database connection pool.
    pool: Pool<ConnectionManager<diesel::PgConnection>>,
}

#[cfg(not(target_os = "wasi"))]
#[cfg(not(tarpaulin_include))]
impl ConcreteUserDao {
    /// Creates a new `ConcreteUserDao` from an existing connection pool.
    pub fn new(pool: Pool<ConnectionManager<diesel::PgConnection>>) -> Self {
        Self { pool }
    }
}

#[cfg(not(target_os = "wasi"))]
#[async_trait]
#[cfg(not(tarpaulin_include))]
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
    use uuid::Uuid;

    #[actix_web::test]
    async fn test_stub_user_dao() {
        let dao = StubUserDao;
        let id = Uuid::new_v4();
        let res = dao.get_user(id).await;
        assert!(matches!(res, Err(ServerError::NotImplemented)));
    }
}
