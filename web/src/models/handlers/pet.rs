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
    use actix_web::{test, web, App, HttpResponse};

    #[actix_web::test]
    async fn test_pet_handler_models_actix_extractors() {
        let app = test::init_service(
            App::new()
                .route(
                    "/form",
                    web::post().to(|f: web::Form<UpdatePetWithFormForm>| async move {
                        HttpResponse::Ok().json(f.into_inner())
                    }),
                )
                .route(
                    "/upload",
                    web::post().to(|q: web::Query<UploadFileQuery>| async move {
                        HttpResponse::Ok().json(q.into_inner())
                    }),
                ),
        )
        .await;

        let q = FindPetsByStatusQuery {
            status: vec!["available".to_string()],
        };
        let json = serde_json::to_string(&q).expect("must serialize");
        let parsed: FindPetsByStatusQuery = serde_json::from_str(&json).expect("must deserialize");
        assert_eq!(parsed, q);

        let req = test::TestRequest::post()
            .uri("/form")
            .set_form(UpdatePetWithFormForm {
                name: Some("Rex".to_string()),
                status: Some("available".to_string()),
            })
            .to_request();
        let extracted_form: UpdatePetWithFormForm = test::call_and_read_body_json(&app, req).await;
        assert_eq!(extracted_form.name.as_deref(), Some("Rex"));

        let req = test::TestRequest::post()
            .uri("/upload?additional_metadata=meta")
            .to_request();
        let extracted_upload: UploadFileQuery = test::call_and_read_body_json(&app, req).await;
        assert_eq!(
            extracted_upload.additional_metadata.as_deref(),
            Some("meta")
        );

        let tags = FindPetsByTagsQuery {
            tags: vec!["tag1".to_string()],
        };
        assert_eq!(tags.tags, vec!["tag1".to_string()]);

        let headers = DeletePetHeaders {
            api_key: Some("k".to_string()),
        };
        assert_eq!(headers.api_key.as_deref(), Some("k"));
    }
}
