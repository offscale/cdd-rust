#![deny(missing_docs)]

//! Pet endpoint parameter models.

use serde::{Deserialize, Serialize};

/// Query parameters for `GET /pet/findByStatus`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct FindPetsByStatusQuery {
    /// Status values that need to be considered for filter.
    pub status: Vec<String>,
}

/// Query parameters for `GET /pet/findByTags`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct FindPetsByTagsQuery {
    /// Tags to filter by.
    pub tags: Vec<String>,
}

/// Form parameters for `POST /pet/{petId}`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct UpdatePetWithFormForm {
    /// Updated name of the pet.
    pub name: Option<String>,
    /// Updated status of the pet.
    pub status: Option<String>,
}

/// Header parameters for `DELETE /pet/{petId}`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DeletePetHeaders {
    /// Optional API key header.
    pub api_key: Option<String>,
}

/// Query parameters for `POST /pet/{petId}/uploadImage`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct UploadFileQuery {
    /// Additional metadata to pass to server.
    pub additional_metadata: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pet_handler_models_serde() {
        let q = FindPetsByStatusQuery {
            status: vec!["available".to_string()],
        };
        let json = serde_json::to_string(&q).expect("must serialize");
        let parsed: FindPetsByStatusQuery = serde_json::from_str(&json).expect("must deserialize");
        assert_eq!(parsed, q);

        let tags = FindPetsByTagsQuery {
            tags: vec!["tag1".to_string()],
        };
        assert_eq!(tags.tags.len(), 1);

        let form = UpdatePetWithFormForm {
            name: Some("Rex".to_string()),
            status: None,
        };
        assert_eq!(form.name.as_deref(), Some("Rex"));

        let headers = DeletePetHeaders { api_key: None };
        assert!(headers.api_key.is_none());

        let upload = UploadFileQuery {
            additional_metadata: Some("meta".to_string()),
        };
        assert_eq!(upload.additional_metadata.as_deref(), Some("meta"));
    }
}
