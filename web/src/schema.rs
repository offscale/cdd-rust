#![deny(missing_docs)]
// @generated automatically by Diesel CLI.
// Manual doc comments added for compliance.

//! Database Schema.

diesel::table! {
    /// The users table.
    users (id) {
        /// Primary Key (UUID).
        id -> Uuid,
        /// Email address.
        email -> Varchar,
        /// Encrypted password.
        password_hash -> Varchar,
        /// Creation timestamp.
        created_at -> Timestamp,
        /// Update timestamp.
        updated_at -> Timestamp,
    }
}

#[cfg(test)]
mod tests {
    use super::users;
    use super::users::dsl::*;
    use diesel::query_builder::QueryFragment;
    use diesel::{QueryDsl, Table};

    fn make_default<T: Default>() -> T {
        T::default()
    }

    #[test]
    fn test_users_schema_table_and_columns() {
        let tbl = users::table;
        let pk = tbl.primary_key();
        let all_cols = users::all_columns;
        let cols_tuple = (id, email, password_hash, created_at, updated_at);
        let default_tbl: users::table = make_default();
        let star_col = users::star;

        let mut query_str = String::new();
        let _ = (tbl, pk, all_cols, cols_tuple, default_tbl, star_col);
        let q = diesel::debug_query::<diesel::pg::Pg, _>(&users.select(all_cols)).to_string();
        query_str.push_str(&q);
        assert!(query_str.contains("users"));
        assert!(
            <users::table as QueryFragment<diesel::pg::Pg>>::is_safe_to_cache_prepared(
                &tbl,
                &diesel::pg::Pg
            )
            .is_ok()
        );
    }
}
