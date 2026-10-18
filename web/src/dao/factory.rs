#![deny(missing_docs)]

//! DAO Factory and dependency injection configurations.

use crate::dao::users::{ConcreteUserDao, StubUserDao, UserDao};
use std::sync::Arc;

#[cfg(not(target_os = "wasi"))]
use diesel::r2d2::{ConnectionManager, Pool};

/// Strategy for DAO instantiation.
#[derive(Clone)]
pub enum DaoConfig {
    /// Returns stubbed `NotImplemented` responses.
    Stub,
    /// Connects to a real PostgreSQL R2D2 connection pool.
    #[cfg(not(target_os = "wasi"))]
    Concrete(Pool<ConnectionManager<diesel::PgConnection>>),
}

/// Container for all application DAOs injected into Actix `web::Data`.
#[derive(Clone)]
pub struct AppDaos {
    /// User Data Access Object.
    pub user_dao: Arc<dyn UserDao>,
}

impl AppDaos {
    /// Create a new [`AppDaos`] container based on the provided [`DaoConfig`] strategy.
    ///
    /// # Arguments
    /// * `config` - DAO instantiation strategy.
    ///
    /// # Returns
    /// * Configured [`AppDaos`] container.
    pub fn new(config: DaoConfig) -> Self {
        match config {
            DaoConfig::Stub => Self {
                user_dao: Arc::new(StubUserDao),
            },
            #[cfg(not(target_os = "wasi"))]
            DaoConfig::Concrete(pool) => Self {
                user_dao: Arc::new(ConcreteUserDao::new(pool)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ServerError;
    use uuid::Uuid;

    #[actix_web::test]
    async fn test_factory_stub_and_concrete() {
        let daos = AppDaos::new(DaoConfig::Stub);
        let res = daos.user_dao.get_user(Uuid::new_v4()).await;
        assert!(matches!(res, Err(ServerError::NotImplemented)));

        let manager =
            ConnectionManager::<diesel::PgConnection>::new("postgres://localhost:5432/cdd");
        let pool = Pool::builder()
            .max_size(1)
            .connection_timeout(std::time::Duration::from_millis(50))
            .build_unchecked(manager);
        let concrete_daos = AppDaos::new(DaoConfig::Concrete(pool));
        let found = concrete_daos
            .user_dao
            .get_user(Uuid::new_v4())
            .await
            .expect("concrete dao query");
        assert!(found.is_none());
    }
}
