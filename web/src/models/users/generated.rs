/* @generated and managed by dsync */

#[allow(unused)]
use crate::diesel::*;
use crate::schema::*;

/// Struct representing a row in table `users`
#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    diesel::Queryable,
    diesel::Selectable,
    diesel::QueryableByName,
    diesel::Identifiable,
)]
#[diesel(table_name=users, primary_key(id))]
pub struct Users {
    /// Field representing column `id`
    pub id: uuid::Uuid,
    /// Field representing column `email`
    pub email: String,
    /// Field representing column `password_hash`
    pub password_hash: String,
    /// Field representing column `created_at`
    pub created_at: chrono::NaiveDateTime,
    /// Field representing column `updated_at`
    pub updated_at: chrono::NaiveDateTime,
}

/// Create Struct for a row in table `users` for [`Users`]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, diesel::Insertable)]
#[diesel(table_name=users)]
pub struct CreateUsers {
    /// Field representing column `id`
    pub id: uuid::Uuid,
    /// Field representing column `email`
    pub email: String,
    /// Field representing column `password_hash`
    pub password_hash: String,
    /// Field representing column `created_at`
    pub created_at: chrono::NaiveDateTime,
    /// Field representing column `updated_at`
    pub updated_at: chrono::NaiveDateTime,
}

/// Update Struct for a row in table `users` for [`Users`]
#[derive(
    Debug, Clone, serde::Serialize, serde::Deserialize, diesel::AsChangeset, PartialEq, Default,
)]
#[diesel(table_name=users)]
pub struct UpdateUsers {
    /// Field representing column `email`
    pub email: Option<String>,
    /// Field representing column `password_hash`
    pub password_hash: Option<String>,
    /// Field representing column `created_at`
    pub created_at: Option<chrono::NaiveDateTime>,
    /// Field representing column `updated_at`
    pub updated_at: Option<chrono::NaiveDateTime>,
}
