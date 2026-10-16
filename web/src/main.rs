#![warn(missing_docs)]
#![deny(missing_docs)]

//! # Web App Entrypoint
//!
//! CLI and HTTP server runner for the CDD reference web server.

#[cfg(not(target_os = "wasi"))]
use actix_web::{web, App};
#[cfg(not(target_os = "wasi"))]
use cdd_web::{
    config,
    dao::{
        connection::{create_pool, DbConfig},
        factory::{AppDaos, DaoConfig},
    },
    health_check,
    idp::configure_idp,
    validation::StrictValidation,
    webhooks,
};
use clap::Parser;
use std::env;

/// Command-line arguments for the CDD Web Server.
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Run the server in stub mode (returns 501 Not Implemented for DAO operations).
    #[arg(long)]
    pub stub: bool,

    /// Create an ephemeral database on startup.
    #[arg(long)]
    pub ephemeral_db: bool,

    /// Seed the database with fake data on startup.
    #[arg(long)]
    pub seed: bool,
}

/// Resolves the [`DaoConfig`] from CLI flags and environment configuration.
///
/// # Arguments
/// * `cli` - Parsed command-line arguments.
///
/// # Returns
/// * `Result<DaoConfig, cdd_web::error::ServerError>` with the configured DAO strategy.
///
/// # Errors
/// * Returns [`cdd_web::error::ServerError`] if concrete pool creation fails.
#[cfg(not(target_os = "wasi"))]
pub fn build_dao_config(cli: &Cli) -> Result<DaoConfig, cdd_web::error::ServerError> {
    if cli.stub {
        Ok(DaoConfig::Stub)
    } else {
        let mut db_config = DbConfig::from_env();
        if cli.ephemeral_db {
            db_config.is_ephemeral = true;
        }
        create_pool(&db_config).map(DaoConfig::Concrete)
    }
}

/// Resolves the `(host, port)` bind tuple from `HOST` and `PORT` environment variables.
///
/// # Returns
/// * `(String, u16)` bind address and port (defaults to `("127.0.0.1", 8080)`).
pub fn resolve_bind_addr() -> (String, u16) {
    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .unwrap_or(8080);
    (host, port)
}

/// Entrypoint for the Actix Web server on non-WASI targets.
#[cfg(not(target_os = "wasi"))]
#[cfg(not(test))]
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    let dao_config = build_dao_config(&cli)
        .map_err(|e| std::io::Error::other(format!("Failed to initialize DB pool: {e}")))?;

    let app_daos = AppDaos::new(dao_config);

    if cli.seed {
        if let Err(e) = cdd_web::seeder::seed_database(app_daos.user_dao.clone()).await {
            eprintln!("Failed to seed database: {e}");
        }
    }

    let daos_data = web::Data::new(app_daos);
    let pet_store = web::Data::new(cdd_web::handlers::pet::PetStore::default());
    let order_store = web::Data::new(cdd_web::handlers::store::OrderStore::default());
    let (host, port) = resolve_bind_addr();

    actix_web::HttpServer::new(move || {
        let cors = actix_cors::Cors::permissive();
        App::new()
            .wrap(cors)
            .wrap(StrictValidation)
            .app_data(daos_data.clone())
            .app_data(pet_store.clone())
            .app_data(order_store.clone())
            .service(health_check)
            .configure(config)
            .configure(configure_idp)
            .configure(webhooks::config)
    })
    .bind((host, port))?
    .run()
    .await
}

#[cfg(target_os = "wasi")]
fn main() {
    println!(
        "CDD Web Server cannot run natively as an HTTP server on WASI without a runtime interface."
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test;

    #[actix_web::test]
    async fn test_cli_parsing_and_bind_addr() {
        let cli = Cli::parse_from(["cdd-web", "--stub", "--ephemeral-db", "--seed"]);
        assert!(cli.stub);
        assert!(cli.ephemeral_db);
        assert!(cli.seed);

        let dao_cfg = build_dao_config(&cli);
        assert!(matches!(dao_cfg, Ok(DaoConfig::Stub)));

        env::set_var("DATABASE_URL", "invalid_url");
        let cli_concrete = Cli::parse_from(["cdd-web", "--ephemeral-db"]);
        assert!(build_dao_config(&cli_concrete).is_err());
        let cli_non_ephemeral = Cli::parse_from(["cdd-web"]);
        assert!(build_dao_config(&cli_non_ephemeral).is_err());
        env::remove_var("DATABASE_URL");

        env::remove_var("HOST");
        env::remove_var("PORT");
        assert_eq!(resolve_bind_addr(), ("127.0.0.1".to_string(), 8080));

        env::set_var("HOST", "0.0.0.0");
        env::set_var("PORT", "9090");
        assert_eq!(resolve_bind_addr(), ("0.0.0.0".to_string(), 9090));
        env::remove_var("HOST");
        env::remove_var("PORT");
    }

    #[actix_web::test]
    async fn test_app_wiring_and_cors() {
        let app_daos = AppDaos::new(DaoConfig::Stub);
        let daos_data = web::Data::new(app_daos);
        let pet_store = web::Data::new(cdd_web::handlers::pet::PetStore::default());
        let order_store = web::Data::new(cdd_web::handlers::store::OrderStore::default());

        let app = test::init_service(
            App::new()
                .wrap(actix_cors::Cors::permissive())
                .wrap(StrictValidation)
                .app_data(daos_data)
                .app_data(pet_store)
                .app_data(order_store)
                .service(health_check)
                .configure(config)
                .configure(configure_idp)
                .configure(webhooks::config),
        )
        .await;

        let req = test::TestRequest::default()
            .method(actix_web::http::Method::OPTIONS)
            .uri("/pet")
            .insert_header(("Origin", "http://example.com"))
            .insert_header(("Access-Control-Request-Method", "GET"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        assert!(resp.headers().contains_key("access-control-allow-origin"));
    }
}
