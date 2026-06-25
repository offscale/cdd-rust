//! DAO Factory and DI configurations.

use crate::dao::users::{ConcreteUserDao, StubUserDao, UserDao};
use std::sync::Arc;

#[cfg(not(target_os = "wasi"))]
use diesel::r2d2::{ConnectionManager, Pool};

/// Strategy for DAO instantiation.
pub enum DaoConfig {
    /// Use stub DAOs that return `NotImplementedError`.
    Stub,
    /// Use concrete DAOs backed by a Postgres database.
    #[cfg(not(target_os = "wasi"))]
    Concrete(Pool<ConnectionManager<diesel::PgConnection>>),
}

/// A container for all DAOs.
#[derive(Clone)]
pub struct AppDaos {
    /// The user DAO.
    pub user_dao: Arc<dyn UserDao>,
}

impl AppDaos {
    /// Construct DAOs based on the provided configuration.
    #[cfg(not(tarpaulin_include))]
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

    #[test]
    fn test_factory_stub() {
        let daos = AppDaos::new(DaoConfig::Stub);
        // Ensure it's constructed.
        let _ = daos.user_dao;
    }
}
