#![deny(missing_docs)]

//! Domain and API data transfer models for the CDD Web Server.

use serde::{Deserialize, Serialize};

/// Category classification for a [`Pet`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    /// Numeric identifier of the category.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    /// Display name of the category.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Tag attached to a [`Pet`], supporting both structured tag objects and plain string labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PetTag {
    /// Structured tag object with optional `id` and `name`.
    Named {
        /// Optional numeric tag identifier.
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<i64>,
        /// Tag name.
        name: String,
    },
    /// Plain string tag label.
    Label(String),
}

impl PetTag {
    /// Returns the tag's string name regardless of representation.
    ///
    /// # Returns
    /// * Tag name slice.
    pub fn name(&self) -> &str {
        match self {
            PetTag::Named { name, .. } => name.as_str(),
            PetTag::Label(s) => s.as_str(),
        }
    }
}

/// Inventory status of a [`Pet`] in the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PetStatus {
    /// Pet is available for purchase.
    Available,
    /// Pet is pending a sale.
    Pending,
    /// Pet has been sold.
    Sold,
}

impl PetStatus {
    /// Returns the canonical lowercase string representation of the status.
    ///
    /// # Returns
    /// * Static status string (`"available"`, `"pending"`, or `"sold"`).
    pub fn as_str(self) -> &'static str {
        match self {
            PetStatus::Available => "available",
            PetStatus::Pending => "pending",
            PetStatus::Sold => "sold",
        }
    }

    /// Parses a status string into a [`PetStatus`].
    ///
    /// # Arguments
    /// * `raw` - Raw status string.
    ///
    /// # Returns
    /// * `Some(PetStatus)` if recognized, `None` otherwise.
    pub fn from_str_opt(raw: &str) -> Option<Self> {
        match raw {
            "available" => Some(PetStatus::Available),
            "pending" => Some(PetStatus::Pending),
            "sold" => Some(PetStatus::Sold),
            _ => None,
        }
    }
}

/// Strongly-typed representation of a Pet entity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pet {
    /// Unique numeric identifier of the pet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    /// Optional category metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<Category>,
    /// Name of the pet.
    #[serde(default)]
    pub name: String,
    /// List of photo URLs for the pet.
    #[serde(rename = "photoUrls", default, skip_serializing_if = "Vec::is_empty")]
    pub photo_urls: Vec<String>,
    /// Tags associated with the pet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<PetTag>,
    /// Store availability status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<PetStatus>,
}

/// Fulfillment status of a store [`Order`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    /// Order has been placed.
    #[default]
    Placed,
    /// Order has been approved.
    Approved,
    /// Order has been delivered.
    Delivered,
}

impl OrderStatus {
    /// Returns the canonical lowercase string representation of the order status.
    ///
    /// # Returns
    /// * Static status string (`"placed"`, `"approved"`, or `"delivered"`).
    pub fn as_str(self) -> &'static str {
        match self {
            OrderStatus::Placed => "placed",
            OrderStatus::Approved => "approved",
            OrderStatus::Delivered => "delivered",
        }
    }
}

/// Strongly-typed representation of a Store Order entity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    /// Unique numeric identifier of the order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    /// Identifier of the ordered pet.
    #[serde(rename = "petId", skip_serializing_if = "Option::is_none")]
    pub pet_id: Option<i64>,
    /// Quantity ordered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<i32>,
    /// Estimated ship timestamp.
    #[serde(rename = "shipDate", skip_serializing_if = "Option::is_none")]
    pub ship_date: Option<String>,
    /// Order status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<OrderStatus>,
    /// Whether the order is complete.
    #[serde(default)]
    pub complete: bool,
}

/// Strongly-typed representation of an OpenAPI User DTO entity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// Optional numeric identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    /// Username.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Given name.
    #[serde(rename = "firstName", skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    /// Family name.
    #[serde(rename = "lastName", skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    /// Email address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Plaintext password for registration/login payloads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Contact phone number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    /// Numeric user status code.
    #[serde(rename = "userStatus", skip_serializing_if = "Option::is_none")]
    pub user_status: Option<i32>,
}

impl User {
    /// Resolves the primary account identifier (preferring `email`, then `username`, or a default).
    ///
    /// # Returns
    /// * Resolved email or username string.
    pub fn resolved_identity(&self) -> String {
        self.email
            .as_deref()
            .or(self.username.as_deref())
            .unwrap_or("user@example.com")
            .to_string()
    }
}

/// Strongly-typed representation of a standard API response envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiResponse {
    /// HTTP or application status code.
    pub code: i32,
    /// Response classification type.
    #[serde(rename = "type")]
    pub type_: String,
    /// Human-readable response message.
    pub message: String,
}

/// Generated route parameter and query models for HTTP handlers.
pub mod handlers;
/// Database-backed User table models.
pub mod users;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_models_methods_and_conversions() {
        let named_tag = PetTag::Named {
            id: Some(1),
            name: "hero".to_string(),
        };
        let label_tag = PetTag::Label("friendly".to_string());
        assert_eq!(named_tag.name(), "hero");
        assert_eq!(label_tag.name(), "friendly");

        assert_eq!(PetStatus::Available.as_str(), "available");
        assert_eq!(PetStatus::Pending.as_str(), "pending");
        assert_eq!(PetStatus::Sold.as_str(), "sold");
        assert_eq!(
            PetStatus::from_str_opt("available"),
            Some(PetStatus::Available)
        );
        assert_eq!(PetStatus::from_str_opt("pending"), Some(PetStatus::Pending));
        assert_eq!(PetStatus::from_str_opt("sold"), Some(PetStatus::Sold));
        assert_eq!(PetStatus::from_str_opt("unknown"), None);

        assert_eq!(OrderStatus::Placed.as_str(), "placed");
        assert_eq!(OrderStatus::Approved.as_str(), "approved");
        assert_eq!(OrderStatus::Delivered.as_str(), "delivered");

        let u_email = User {
            email: Some("a@b.com".to_string()),
            ..Default::default()
        };
        assert_eq!(u_email.resolved_identity(), "a@b.com");
        let u_name = User {
            username: Some("john".to_string()),
            ..Default::default()
        };
        assert_eq!(u_name.resolved_identity(), "john");
        assert_eq!(User::default().resolved_identity(), "user@example.com");
    }
}
