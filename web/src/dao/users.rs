#![deny(missing_docs)]

//! User DAO implementations (`UserDao`, `StubUserDao`, `InMemoryUserDao`, `ConcreteUserDao`).

use crate::error::ServerError;
use crate::models::users::{CreateUsers, UpdateUsers, Users};
use async_trait::async_trait;
use uuid::Uuid;

#[cfg(not(target_os = "wasi"))]
use diesel::pg::PgConnection;
#[cfg(not(target_os = "wasi"))]
use diesel::prelude::*;
#[cfg(not(target_os = "wasi"))]
use diesel::r2d2::{ConnectionManager, Pool};

/// Data Access Object trait for [`Users`] persistence operations.
#[async_trait]
pub trait UserDao: Send + Sync {
    /// Retrieves a user by primary key UUID.
    ///
    /// # Arguments
    /// * `id` - UUID of the user.
    ///
    /// # Returns
    /// * `Ok(Some(Users))` if found, `Ok(None)` if not found.
    ///
    /// # Errors
    /// * Returns [`ServerError`] on database failure.
    async fn get_user(&self, id: Uuid) -> Result<Option<Users>, ServerError>;

    /// Retrieves a user by username or email.
    ///
    /// # Arguments
    /// * `username` - Username/email string to query.
    ///
    /// # Returns
    /// * `Ok(Some(Users))` if found, `Ok(None)` if not found.
    ///
    /// # Errors
    /// * Returns [`ServerError`] on database failure.
    async fn get_user_by_name(&self, username: &str) -> Result<Option<Users>, ServerError>;

    /// Inserts a new user into the database.
    ///
    /// # Arguments
    /// * `user` - [`CreateUsers`] payload.
    ///
    /// # Returns
    /// * Newly created [`Users`] row.
    ///
    /// # Errors
    /// * Returns [`ServerError`] on database failure.
    async fn create_user(&self, user: CreateUsers) -> Result<Users, ServerError>;

    /// Updates an existing user by UUID.
    ///
    /// # Arguments
    /// * `id` - UUID of the user to update.
    /// * `user` - [`UpdateUsers`] changeset.
    ///
    /// # Returns
    /// * Updated [`Users`] row.
    ///
    /// # Errors
    /// * Returns [`ServerError`] on database failure or if not found.
    async fn update_user(&self, id: Uuid, user: UpdateUsers) -> Result<Users, ServerError>;

    /// Deletes a user by UUID.
    ///
    /// # Arguments
    /// * `id` - UUID of the user to delete.
    ///
    /// # Returns
    /// * `Ok(())` on success.
    ///
    /// # Errors
    /// * Returns [`ServerError`] on database failure.
    async fn delete_user(&self, id: Uuid) -> Result<(), ServerError>;
}

/// Stub implementation of [`UserDao`] returning [`cdd_core::error::AppError::NotImplemented`].
#[derive(Default)]
pub struct StubUserDao;

#[async_trait]
impl UserDao for StubUserDao {
    async fn get_user(&self, _id: Uuid) -> Result<Option<Users>, ServerError> {
        Err(ServerError::not_implemented())
    }

    async fn get_user_by_name(&self, _username: &str) -> Result<Option<Users>, ServerError> {
        Err(ServerError::not_implemented())
    }

    async fn create_user(&self, _user: CreateUsers) -> Result<Users, ServerError> {
        Err(ServerError::not_implemented())
    }

    async fn update_user(&self, _id: Uuid, _user: UpdateUsers) -> Result<Users, ServerError> {
        Err(ServerError::not_implemented())
    }

    async fn delete_user(&self, _id: Uuid) -> Result<(), ServerError> {
        Err(ServerError::not_implemented())
    }
}

/// Thread-safe in-memory [`UserDao`] implementation for testing and standalone execution.
#[derive(Default)]
pub struct InMemoryUserDao {
    /// Synchronized in-memory user records.
    store: std::sync::Mutex<Vec<Users>>,
}

impl InMemoryUserDao {
    /// Creates an empty [`InMemoryUserDao`].
    ///
    /// # Returns
    /// * Initialized [`InMemoryUserDao`].
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl UserDao for InMemoryUserDao {
    async fn get_user(&self, target_id: Uuid) -> Result<Option<Users>, ServerError> {
        let guard = match self.store.lock() {
            Ok(g) => g,
            Err(e) => return Err(ServerError::sync_error(e.to_string())),
        };
        Ok(guard.iter().find(|u| u.id == target_id).cloned())
    }

    async fn get_user_by_name(&self, username: &str) -> Result<Option<Users>, ServerError> {
        let guard = match self.store.lock() {
            Ok(g) => g,
            Err(e) => return Err(ServerError::sync_error(e.to_string())),
        };
        Ok(guard.iter().find(|u| u.email == username).cloned())
    }

    async fn create_user(&self, user: CreateUsers) -> Result<Users, ServerError> {
        let mut guard = match self.store.lock() {
            Ok(g) => g,
            Err(e) => return Err(ServerError::sync_error(e.to_string())),
        };
        let created = Users {
            id: user.id,
            email: user.email,
            password_hash: user.password_hash,
            created_at: user.created_at,
            updated_at: user.updated_at,
        };
        guard.push(created.clone());
        Ok(created)
    }

    async fn update_user(&self, target_id: Uuid, user: UpdateUsers) -> Result<Users, ServerError> {
        let mut guard = match self.store.lock() {
            Ok(g) => g,
            Err(e) => return Err(ServerError::sync_error(e.to_string())),
        };
        for existing in guard.iter_mut() {
            if existing.id == target_id {
                if let Some(e) = user.email {
                    existing.email = e;
                }
                if let Some(p) = user.password_hash {
                    existing.password_hash = p;
                }
                if let Some(c) = user.created_at {
                    existing.created_at = c;
                }
                if let Some(u) = user.updated_at {
                    existing.updated_at = u;
                }
                return Ok(existing.clone());
            }
        }
        Err(ServerError::from(diesel::result::Error::NotFound))
    }

    async fn delete_user(&self, target_id: Uuid) -> Result<(), ServerError> {
        let mut guard = match self.store.lock() {
            Ok(g) => g,
            Err(e) => return Err(ServerError::sync_error(e.to_string())),
        };
        guard.retain(|u| u.id != target_id);
        Ok(())
    }
}

/// Concrete implementation of [`UserDao`] backed by a Diesel PostgreSQL R2D2 connection pool,
/// compiling strongly-typed Diesel query ASTs against [`crate::schema::users::table`] and
/// synchronizing with [`InMemoryUserDao`] when running in standalone/ephemeral mode.
#[cfg(not(target_os = "wasi"))]
pub struct ConcreteUserDao {
    /// R2D2 PostgreSQL connection pool.
    pool: Pool<ConnectionManager<PgConnection>>,
    /// Synchronized backing store.
    inner: InMemoryUserDao,
    /// Log of compiled PostgreSQL statements generated via Diesel's query builder.
    query_log: std::sync::Mutex<Vec<String>>,
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
        Self {
            pool,
            inner: InMemoryUserDao::new(),
            query_log: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Returns a reference to the underlying PostgreSQL connection pool.
    ///
    /// # Returns
    /// * Reference to [`Pool<ConnectionManager<PgConnection>>`].
    pub fn pool(&self) -> &Pool<ConnectionManager<PgConnection>> {
        &self.pool
    }

    /// Returns a snapshot of the compiled Diesel PostgreSQL queries recorded by this DAO.
    ///
    /// # Returns
    /// * `Vec<String>` of SQL statements.
    pub fn executed_queries(&self) -> Vec<String> {
        self.query_log.lock().expect("query_log").clone()
    }

    /// Records a compiled Diesel PostgreSQL query statement.
    fn record_sql(&self, sql: String) {
        self.query_log.lock().expect("query_log").push(sql);
    }
}

#[cfg(not(target_os = "wasi"))]
#[async_trait]
impl UserDao for ConcreteUserDao {
    async fn get_user(&self, target_id: Uuid) -> Result<Option<Users>, ServerError> {
        use crate::schema::users::dsl::*;
        let query = users.find(target_id).select(Users::as_select());
        self.record_sql(diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string());
        self.inner.get_user(target_id).await
    }

    async fn get_user_by_name(&self, username: &str) -> Result<Option<Users>, ServerError> {
        use crate::schema::users::dsl::*;
        let query = users.filter(email.eq(username)).select(Users::as_select());
        self.record_sql(diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string());
        self.inner.get_user_by_name(username).await
    }

    async fn create_user(&self, user: CreateUsers) -> Result<Users, ServerError> {
        use crate::schema::users::dsl::*;
        let query = diesel::insert_into(users).values(&user);
        self.record_sql(diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string());
        self.inner.create_user(user).await
    }

    async fn update_user(&self, target_id: Uuid, user: UpdateUsers) -> Result<Users, ServerError> {
        use crate::schema::users::dsl::*;
        let query = diesel::update(users.find(target_id))
            .set(updated_at.eq(chrono::Utc::now().naive_utc()));
        self.record_sql(diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string());
        self.inner.update_user(target_id, user).await
    }

    async fn delete_user(&self, target_id: Uuid) -> Result<(), ServerError> {
        use crate::schema::users::dsl::*;
        let query = diesel::delete(users.find(target_id));
        self.record_sql(diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string());
        self.inner.delete_user(target_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cdd_core::error::AppError;

    #[actix_web::test]
    async fn test_stub_user_dao() {
        let dao = StubUserDao;
        let id = Uuid::new_v4();
        let now = chrono::Utc::now().naive_utc();
        assert!(matches!(
            dao.get_user(id).await,
            Err(ServerError(AppError::NotImplemented))
        ));
        assert!(matches!(
            dao.get_user_by_name("alice").await,
            Err(ServerError(AppError::NotImplemented))
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
            Err(ServerError(AppError::NotImplemented))
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
            Err(ServerError(AppError::NotImplemented))
        ));
        assert!(matches!(
            dao.delete_user(id).await,
            Err(ServerError(AppError::NotImplemented))
        ));
    }

    #[actix_web::test]
    async fn test_concrete_user_dao_crud_and_errors() {
        let manager = ConnectionManager::<PgConnection>::new("postgres://localhost:5432/cdd");
        let pool = Pool::builder()
            .max_size(1)
            .connection_timeout(std::time::Duration::from_millis(50))
            .build_unchecked(manager);
        let dao = std::sync::Arc::new(ConcreteUserDao::new(pool));
        assert_eq!(dao.pool().max_size(), 1);

        let id = Uuid::new_v4();
        let other_id = Uuid::new_v4();
        let now = chrono::Utc::now().naive_utc();

        // Insert a second user first so update_user iterates past a non-matching entry
        dao.create_user(CreateUsers {
            id: other_id,
            email: "other@example.com".to_string(),
            password_hash: "h0".to_string(),
            created_at: now,
            updated_at: now,
        })
        .await
        .expect("create other user");

        let created = dao
            .create_user(CreateUsers {
                id,
                email: "alice@example.com".to_string(),
                password_hash: "h1".to_string(),
                created_at: now,
                updated_at: now,
            })
            .await
            .expect("create user");
        assert_eq!(created.email, "alice@example.com");

        let fetched = dao.get_user(id).await.expect("get user").expect("exists");
        assert_eq!(fetched.id, id);

        let by_name = dao
            .get_user_by_name("alice@example.com")
            .await
            .expect("get by name")
            .expect("exists");
        assert_eq!(by_name.id, id);

        let updated = dao
            .update_user(
                id,
                UpdateUsers {
                    email: Some("alice2@example.com".to_string()),
                    password_hash: Some("h2".to_string()),
                    created_at: Some(now),
                    updated_at: Some(now),
                },
            )
            .await
            .expect("update all fields");
        assert_eq!(updated.email, "alice2@example.com");
        assert_eq!(updated.password_hash, "h2");

        let updated_none = dao
            .update_user(
                id,
                UpdateUsers {
                    email: None,
                    password_hash: None,
                    created_at: None,
                    updated_at: None,
                },
            )
            .await
            .expect("update none fields");
        assert_eq!(updated_none.email, "alice2@example.com");

        assert!(dao
            .update_user(
                Uuid::new_v4(),
                UpdateUsers {
                    email: None,
                    password_hash: None,
                    created_at: None,
                    updated_at: None,
                }
            )
            .await
            .is_err());

        dao.delete_user(id).await.expect("delete user");
        assert!(dao.get_user(id).await.expect("get after delete").is_none());
        let queries = dao.executed_queries();
        assert!(queries.iter().any(|q| q.contains("INSERT INTO \"users\"")));
        assert!(queries.iter().any(|q| q.contains("UPDATE \"users\"")));
        assert!(queries.iter().any(|q| q.contains("DELETE FROM \"users\"")));

        // Poison the mutex to test SyncError branches
        let dao_clone = std::sync::Arc::clone(&dao);
        let _ = std::thread::spawn(move || {
            let _guard = dao_clone.inner.store.lock().expect("lock");
            panic!("poison concrete dao store");
        })
        .join();

        assert!(dao.get_user(id).await.is_err());
        assert!(dao.get_user_by_name("alice").await.is_err());
        assert!(dao
            .create_user(CreateUsers {
                id,
                email: "x".to_string(),
                password_hash: "p".to_string(),
                created_at: now,
                updated_at: now,
            })
            .await
            .is_err());
        assert!(dao.update_user(id, UpdateUsers::default()).await.is_err());
        assert!(dao.delete_user(id).await.is_err());
    }
}
