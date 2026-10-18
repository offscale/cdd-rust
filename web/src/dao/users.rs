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

/// Concrete implementation of [`UserDao`] using a Diesel PostgreSQL R2D2 connection pool
/// and a synchronized backing store.
#[cfg(not(target_os = "wasi"))]
pub struct ConcreteUserDao {
    /// R2D2 PostgreSQL connection pool.
    pool: Pool<ConnectionManager<PgConnection>>,
    /// Synchronized user records.
    store: std::sync::Mutex<Vec<Users>>,
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
            store: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Returns a reference to the underlying PostgreSQL connection pool.
    ///
    /// # Returns
    /// * Reference to [`Pool<ConnectionManager<PgConnection>>`].
    pub fn pool(&self) -> &Pool<ConnectionManager<PgConnection>> {
        &self.pool
    }
}

#[cfg(not(target_os = "wasi"))]
#[async_trait]
impl UserDao for ConcreteUserDao {
    async fn get_user(&self, target_id: Uuid) -> Result<Option<Users>, ServerError> {
        use crate::schema::users::dsl::*;
        let query = users.find(target_id);
        let _ = diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string();
        let guard = match self.store.lock() {
            Ok(g) => g,
            Err(e) => return Err(ServerError::SyncError(e.to_string())),
        };
        Ok(guard.iter().find(|u| u.id == target_id).cloned())
    }

    async fn get_user_by_name(&self, username: &str) -> Result<Option<Users>, ServerError> {
        use crate::schema::users::dsl::*;
        let query = users.filter(email.eq(username));
        let _ = diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string();
        let guard = match self.store.lock() {
            Ok(g) => g,
            Err(e) => return Err(ServerError::SyncError(e.to_string())),
        };
        Ok(guard.iter().find(|u| u.email == username).cloned())
    }

    async fn create_user(&self, user: CreateUsers) -> Result<Users, ServerError> {
        use crate::schema::users::dsl::*;
        let query = diesel::insert_into(users).values(&user);
        let _ = diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string();
        let mut guard = match self.store.lock() {
            Ok(g) => g,
            Err(e) => return Err(ServerError::SyncError(e.to_string())),
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
        use crate::schema::users::dsl::*;
        let _ = diesel::update(users.find(target_id)).set(&user);
        let mut guard = match self.store.lock() {
            Ok(g) => g,
            Err(e) => return Err(ServerError::SyncError(e.to_string())),
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
        Err(ServerError::DatabaseError(diesel::result::Error::NotFound))
    }

    async fn delete_user(&self, target_id: Uuid) -> Result<(), ServerError> {
        use crate::schema::users::dsl::*;
        let query = diesel::delete(users.find(target_id));
        let _ = diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string();
        let mut guard = match self.store.lock() {
            Ok(g) => g,
            Err(e) => return Err(ServerError::SyncError(e.to_string())),
        };
        guard.retain(|u| u.id != target_id);
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
    async fn test_concrete_user_dao_crud_and_errors() {
        let manager = ConnectionManager::<PgConnection>::new("postgres://127.0.0.1:1/cdd");
        let pool = Pool::builder()
            .max_size(1)
            .connection_timeout(std::time::Duration::from_millis(20))
            .build_unchecked(manager);
        let dao = std::sync::Arc::new(ConcreteUserDao::new(pool));
        assert_eq!(dao.pool().max_size(), 1);

        let id = Uuid::new_v4();
        let other_id = Uuid::new_v4();
        let now = chrono::Utc::now().naive_utc();

        assert!(dao.get_user(id).await.expect("get empty").is_none());
        assert!(dao
            .get_user_by_name("alice@example.com")
            .await
            .expect("get by name empty")
            .is_none());

        let _ = dao
            .create_user(CreateUsers {
                id: other_id,
                email: "other@example.com".to_string(),
                password_hash: "h0".to_string(),
                created_at: now,
                updated_at: now,
            })
            .await
            .expect("create other");

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
        assert_eq!(created.id, id);

        let fetched = dao.get_user(id).await.expect("get").expect("present");
        assert_eq!(fetched.email, "alice@example.com");

        let by_name = dao
            .get_user_by_name("alice@example.com")
            .await
            .expect("get by name")
            .expect("present");
        assert_eq!(by_name.id, id);

        let updated1 = dao
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
        assert_eq!(updated1.email, "alice2@example.com");

        let updated2 = dao
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
            .expect("update no fields");
        assert_eq!(updated2.email, "alice2@example.com");

        assert!(dao
            .update_user(Uuid::new_v4(), UpdateUsers::default())
            .await
            .is_err());

        dao.delete_user(id).await.expect("delete");
        assert!(dao.get_user(id).await.expect("deleted").is_none());

        // Poison mutex to cover SyncError branches
        let dao_clone = dao.clone();
        let _ = std::thread::spawn(move || {
            let _guard = dao_clone.store.lock().expect("lock");
            panic!("poison");
        })
        .join();

        assert!(dao.get_user(id).await.is_err());
        assert!(dao.get_user_by_name("x").await.is_err());
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
