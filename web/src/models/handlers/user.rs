#![deny(missing_docs)]

//! User endpoint parameter models.

use serde::{Deserialize, Serialize};

/// Query parameters for `GET /user/login`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LoginUserQuery {
    /// The user name for login.
    pub username: String,
    /// The password for login in clear text.
    pub password: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_user_query() {
        let q = LoginUserQuery {
            username: "alice".to_string(),
            password: "secret".to_string(),
        };
        let s = serde_json::to_string(&q).expect("must serialize");
        let d: LoginUserQuery = serde_json::from_str(&s).expect("must deserialize");
        assert_eq!(d, q);
    }
}
