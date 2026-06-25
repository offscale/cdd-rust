#![warn(missing_docs)]
#![deny(missing_docs)]
//! # Web App
#![cfg(not(tarpaulin_include))]

#[cfg(not(target_os = "wasi"))]
use actix_web::{web, App, HttpServer};
#[cfg(not(target_os = "wasi"))]
use cdd_web::{
    config,
    dao::{
        connection::{create_pool, DbConfig},
        factory::{AppDaos, DaoConfig},
    },
    health_check,
    seeder::seed_database,
};
#[cfg(not(target_os = "wasi"))]
use clap::Parser;
#[cfg(not(target_os = "wasi"))]
use std::net::TcpListener;
#[cfg(not(target_os = "wasi"))]
use std::sync::Mutex;

/// CDD Mock Server CLI Configuration.
#[cfg(not(target_os = "wasi"))]
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Triggers the Concrete DAOs and overrides DATABASE_URL with a throwaway database.
    #[arg(long)]
    pub ephemeral: bool,

    /// Runs the fake data seeder on startup (requires a concrete DB connection).
    #[arg(long)]
    pub seed: bool,

    /// Enables strict request validation against constraints.
    #[arg(long)]
    pub strict_validation: bool,

    /// Toggles mock security. Validates against mock tokens if active.
    #[arg(long)]
    pub enforce_auth: bool,

    /// Start the integrated Identity Provider (IdP) module alongside the main application.
    #[arg(long)]
    pub start_auth_server: bool,
}

#[cfg(not(target_os = "wasi"))]
fn build_server(
    listener: TcpListener,
    daos: AppDaos,
    strict_validation: bool,
    enforce_auth: bool,
    is_ephemeral: bool,
    has_db: bool,
    start_auth_server: bool,
) -> std::io::Result<actix_web::dev::Server> {
    // For backwards compatibility of pet store code currently present
    let pet_store = web::Data::new(cdd_web::handlers::pet::PetStore {
        pets: Mutex::new(std::collections::HashMap::new()),
    });

    let app_daos = web::Data::new(daos.clone());

    Ok(HttpServer::new(move || {
        let mut app = App::new()
            .wrap(actix_cors::Cors::permissive())
            .wrap(cdd_web::security::HybridAuth {
                enforce_auth,
                is_mock_mode: is_ephemeral || !has_db,
                user_dao: daos.user_dao.clone(),
            })
            .wrap(cdd_web::validation::StrictValidation {
                enabled: strict_validation,
            })
            .app_data(pet_store.clone())
            .app_data(app_daos.clone())
            .service(health_check)
            .configure(config)
            .configure(cdd_web::webhooks::config);

        if start_auth_server {
            app = app.configure(cdd_web::idp::config);
        }

        app
    })
    .listen(listener)?
    .run())
}

#[cfg(not(target_os = "wasi"))]
fn resolve_bind_addr() -> String {
    std::env::var("CDD_WEB_BIND").unwrap_or_else(|_| "localhost:8080".to_string())
}

#[cfg(not(target_os = "wasi"))]
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    // 1. Resolve configuration
    let mut db_config = DbConfig::from_env();
    if cli.ephemeral {
        db_config.is_ephemeral = true;
    }

    // 2. Database Initialization & DAOs
    let has_db = !db_config.database_url.is_empty();

    let dao_config = if has_db || cli.ephemeral {
        match create_pool(&db_config) {
            Ok(pool) => DaoConfig::Concrete(pool),
            Err(e) => {
                eprintln!("Failed to create DB pool: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        DaoConfig::Stub
    };

    let daos = AppDaos::new(dao_config);

    // 3. Data Seeding
    if cli.seed {
        if !has_db && !cli.ephemeral {
            eprintln!("Error: Cannot seed database without a database connection.");
            std::process::exit(1);
        }
        match seed_database(daos.user_dao.clone()).await {
            Ok(_) => println!("Successfully seeded the database."),
            Err(e) => {
                eprintln!("Failed to seed database: {}", e);
                std::process::exit(1);
            }
        }
    }

    // 4. Start Listeners
    let bind_addr = resolve_bind_addr();
    let listener = TcpListener::bind(bind_addr)?;
    let server = build_server(
        listener,
        daos,
        cli.strict_validation,
        cli.enforce_auth,
        cli.ephemeral,
        has_db,
        cli.start_auth_server,
    )?;

    if std::env::var("CDD_WEB_ONESHOT").is_ok() {
        let handle = server.handle();
        let server_task = actix_web::rt::spawn(server);
        handle.stop(true).await;
        let _ = server_task.await;
        return Ok(());
    }

    server.await
}

#[cfg(target_os = "wasi")]
fn main() -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_os = "wasi"))]
    use super::*;

    #[cfg(not(target_os = "wasi"))]
    #[test]
    fn test_cli_parser_ephemeral() {
        use clap::Parser;
        let cli = Cli::parse_from(["cdd-web", "--ephemeral"]);
        assert!(cli.ephemeral);
        assert!(!cli.seed);
    }

    #[cfg(not(target_os = "wasi"))]
    #[test]
    fn test_cli_parser_seed() {
        use clap::Parser;
        let cli = Cli::parse_from(["cdd-web", "--seed"]);
        assert!(!cli.ephemeral);
        assert!(cli.seed);
    }

    #[cfg(not(target_os = "wasi"))]
    #[actix_web::test]
    async fn test_cors_preflight() {
        use actix_web::{test, App};
        use cdd_web::{config, health_check};
        use std::collections::HashMap;
        use std::sync::Mutex;

        let pet_store = web::Data::new(cdd_web::handlers::pet::PetStore {
            pets: Mutex::new(HashMap::new()),
        });

        let daos = AppDaos::new(DaoConfig::Stub);
        let app_daos = web::Data::new(daos);

        let app = test::init_service(
            App::new()
                .wrap(actix_cors::Cors::permissive())
                .app_data(pet_store)
                .app_data(app_daos)
                .service(health_check)
                .configure(config),
        )
        .await;

        let req = test::TestRequest::default()
            .method(actix_web::http::Method::OPTIONS)
            .uri("/health")
            .insert_header(("Origin", "http://example.com"))
            .insert_header(("Access-Control-Request-Method", "GET"))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        assert!(resp.headers().contains_key("access-control-allow-origin"));
    }
}
