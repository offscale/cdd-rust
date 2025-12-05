use chrono::Utc;
use uuid::Uuid;

#[test]
fn test_users_struct_integrity() {
    // We access the struct. The path depends on how dsync generated the module structure.
    // Standard dsync behavior for table `users` -> module `users` -> struct `Users`.
    // Also checks that `cdd_web::diesel` is available if the generated code uses it.

    use cdd_web::models::users::Users;

    let user = Users {
        id: Uuid::new_v4(),
        email: "test@example.com".to_string(),
        password_hash: "secret".to_string(),
        created_at: Utc::now().naive_utc(),
        updated_at: Utc::now().naive_utc(),
    };

    assert_eq!(user.email, "test@example.com");
}
