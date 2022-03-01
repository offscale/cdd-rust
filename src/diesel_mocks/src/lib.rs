pub mod models;
pub mod schema;

use std::env;
use diesel::prelude::*;

pub fn establish_connection() -> SqliteConnection {
    let database_url = env::var("DATABASE_URL").unwrap_or(String::from(":memory:"));
    SqliteConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}
