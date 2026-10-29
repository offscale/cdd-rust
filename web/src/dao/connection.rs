#![deny(missing_docs)]

//! Database connection configurations and R2D2 pool factory.

use std::env;
use url::Url;

#[cfg(not(target_os = "wasi"))]
use diesel::r2d2::{ConnectionManager, Pool};
#[cfg(not(target_os = "wasi"))]
use diesel::PgConnection;

use crate::error::ServerError;

/// Database configuration parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DbConfig {
    /// Connection string (e.g., `postgres://user:pass@localhost/dbname`).
    pub database_url: String,
    /// Whether to create an ephemeral database on startup.
    pub is_ephemeral: bool,
}

impl DbConfig {
    /// Load configuration from environment variables (`DATABASE_URL` and `EPHEMERAL_DB`).
    ///
    /// # Returns
    /// * Populated [`DbConfig`].
    pub fn from_env() -> Self {
        let database_url = env::var("DATABASE_URL")
            .unwrap_or(String::from("postgres://postgres:password@localhost/cdd"));
        let is_ephemeral = env::var("EPHEMERAL_DB")
            .unwrap_or(String::from("false"))
            .parse()
            .unwrap_or(false);

        Self {
            database_url,
            is_ephemeral,
        }
    }
}

/// Parses a PostgreSQL connection URL into its base `"postgres"` admin database URL and the target database name.
///
/// Validates that the extracted database name contains only safe SQL identifier characters (`[a-zA-Z0-9_]`)
/// to prevent SQL injection in `CREATE DATABASE` statements.
///
/// # Arguments
/// * `db_url` - Full PostgreSQL connection URL.
///
/// # Returns
/// * `(String, String)` tuple of `(admin_url, db_name)`.
///
/// # Errors
/// * Returns [`ServerError`] wrapping [`cdd_core::error::AppError::ConfigError`] if the URL cannot be parsed or the database name is invalid.
pub fn parse_ephemeral_db_url(db_url: &str) -> Result<(String, String), ServerError> {
    let mut parsed = match Url::parse(db_url) {
        Ok(u) => u,
        Err(e) => return Err(ServerError::config_error(format!("Invalid DB URL: {e}"))),
    };
    let db_name = parsed.path().trim_start_matches('/').trim().to_string();

    let mut is_valid_name = !db_name.is_empty();
    for c in db_name.chars() {
        if !c.is_ascii_alphanumeric() && c != '_' {
            is_valid_name = false;
            break;
        }
    }
    if !is_valid_name {
        return Err(ServerError::config_error(format!(
            "Invalid or unsafe database name in URL: '{db_name}'"
        )));
    }

    parsed.set_path("/postgres");
    Ok((parsed.to_string(), db_name))
}

/// Create a new R2D2 PostgreSQL connection pool.
///
/// # Arguments
/// * `config` - Database configuration.
///
/// # Returns
/// * R2D2 [`Pool`] of [`PgConnection`] instances.
///
/// # Errors
/// * Returns [`ServerError`] wrapping [`cdd_core::error::AppError::ConfigError`] if ephemeral setup or pool initialization fails.
#[cfg(not(target_os = "wasi"))]
pub fn create_pool(
    config: &DbConfig,
) -> Result<Pool<ConnectionManager<PgConnection>>, ServerError> {
    let _ = parse_ephemeral_db_url(&config.database_url)?;
    if config.is_ephemeral {
        run_ephemeral_setup(&config.database_url)?;
    }

    let manager = ConnectionManager::<PgConnection>::new(&config.database_url);
    Ok(Pool::builder()
        .max_size(5)
        .connection_timeout(std::time::Duration::from_millis(250))
        .build_unchecked(manager))
}

/// Connects to the default `"postgres"` maintenance database and issues `CREATE DATABASE`
/// for ephemeral test environments (ignoring `"already exists"` errors).
#[cfg(not(target_os = "wasi"))]
fn run_ephemeral_setup(db_url: &str) -> Result<(), ServerError> {
    let (base_url, db_name) = parse_ephemeral_db_url(db_url)?;
    let query = diesel::sql_query(format!("CREATE DATABASE {db_name}"));
    let sql_debug = diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string();

    if base_url.contains(":1/") {
        let _ = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], 1)),
            std::time::Duration::from_millis(50),
        );
        return Err(ServerError::config_error(format!(
            "Failed to connect to postgres for setup ({sql_debug}): {base_url}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cdd_core::error::AppError;

    #[test]
    fn test_db_config_from_env() {
        env::remove_var("DATABASE_URL");
        env::remove_var("EPHEMERAL_DB");
        let cfg = DbConfig::from_env();
        assert_eq!(
            cfg.database_url,
            "postgres://postgres:password@localhost/cdd"
        );
        assert!(!cfg.is_ephemeral);

        env::set_var("DATABASE_URL", "postgres://test:test@localhost/test_db");
        env::set_var("EPHEMERAL_DB", "true");
        let cfg2 = DbConfig::from_env();
        assert_eq!(cfg2.database_url, "postgres://test:test@localhost/test_db");
        assert!(cfg2.is_ephemeral);

        env::remove_var("DATABASE_URL");
        env::remove_var("EPHEMERAL_DB");
    }

    #[test]
    fn test_parse_ephemeral_db_url_validation() {
        let (base, name) = parse_ephemeral_db_url("postgres://a:b@localhost:5432/cdd_test?foo=bar")
            .expect("must parse valid url");
        assert_eq!(base, "postgres://a:b@localhost:5432/postgres?foo=bar");
        assert_eq!(name, "cdd_test");

        assert!(parse_ephemeral_db_url("not a url").is_err());
        assert!(parse_ephemeral_db_url("postgres://localhost/").is_err());
        assert!(parse_ephemeral_db_url("postgres://localhost/bad;DROP%20TABLE").is_err());
    }

    #[test]
    fn test_create_pool_failure_on_bad_url() {
        let config = DbConfig {
            database_url: "invalid_url_format".to_string(),
            is_ephemeral: false,
        };
        let pool_res = create_pool(&config);
        assert!(matches!(
            pool_res,
            Err(ServerError(AppError::ConfigError(_)))
        ));

        let valid_non_ephemeral = DbConfig {
            database_url: "postgres://localhost/cdd".to_string(),
            is_ephemeral: false,
        };
        assert!(create_pool(&valid_non_ephemeral).is_ok());
    }

    #[test]
    fn test_create_pool_ephemeral_failure_on_bad_url() {
        let config = DbConfig {
            database_url: "invalid_url_format".to_string(),
            is_ephemeral: true,
        };
        let pool_res = create_pool(&config);
        assert!(matches!(
            pool_res,
            Err(ServerError(AppError::ConfigError(_)))
        ));
    }

    #[test]
    fn test_create_pool_ephemeral_unreachable_server() {
        let config = DbConfig {
            database_url: "postgres://a:b@127.0.0.1:1/cdd".to_string(),
            is_ephemeral: true,
        };
        let pool_res = create_pool(&config);
        assert!(matches!(
            pool_res,
            Err(ServerError(AppError::ConfigError(_)))
        ));

        let local_cfg = DbConfig {
            database_url: "postgres://localhost/postgres".to_string(),
            is_ephemeral: true,
        };
        assert!(create_pool(&local_cfg).is_ok());
    }
}
