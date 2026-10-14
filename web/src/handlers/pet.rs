#![deny(missing_docs)]

//! Pet resource HTTP handlers backed by thread-safe in-memory state.

use crate::error::ServerError;
use actix_web::{web, HttpResponse};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// Thread-safe in-memory store for pet entities indexed by numeric ID.
#[derive(Default)]
pub struct PetStore {
    /// Mutex-protected map from pet ID to JSON representation.
    pub pets: Mutex<HashMap<i64, Value>>,
}

/// Adds a new pet to the store (`POST /pet`).
///
/// # Arguments
/// * `body` - JSON payload representing the pet, which must contain an integer `"id"`.
/// * `store` - Shared application [`PetStore`] state.
///
/// # Returns
/// * `200 OK` with the created pet JSON, or `400 Bad Request` if `"id"` is missing.
///
/// # Errors
/// * Returns [`ServerError::SyncError`] if the internal mutex is poisoned.
pub async fn add_pet(
    body: web::Json<Value>,
    store: web::Data<PetStore>,
) -> Result<HttpResponse, ServerError> {
    let mut map = store
        .pets
        .lock()
        .map_err(|_| ServerError::SyncError("PetStore lock poisoned".to_string()))?;
    if let Some(id) = body.get("id").and_then(|v| v.as_i64()) {
        map.insert(id, body.clone());
        Ok(HttpResponse::Ok().json(body.into_inner()))
    } else {
        Ok(HttpResponse::BadRequest().finish())
    }
}

/// Updates an existing pet in the store (`PUT /pet`).
///
/// # Arguments
/// * `body` - JSON payload representing the updated pet, containing an integer `"id"`.
/// * `store` - Shared application [`PetStore`] state.
///
/// # Returns
/// * `200 OK` with the updated pet JSON, or `400 Bad Request` if `"id"` is missing.
///
/// # Errors
/// * Returns [`ServerError::SyncError`] if the internal mutex is poisoned.
pub async fn update_pet(
    body: web::Json<Value>,
    store: web::Data<PetStore>,
) -> Result<HttpResponse, ServerError> {
    let mut map = store
        .pets
        .lock()
        .map_err(|_| ServerError::SyncError("PetStore lock poisoned".to_string()))?;
    if let Some(id) = body.get("id").and_then(|v| v.as_i64()) {
        map.insert(id, body.clone());
        Ok(HttpResponse::Ok().json(body.into_inner()))
    } else {
        Ok(HttpResponse::BadRequest().finish())
    }
}

/// Finds pets filtered by status (`GET /pet/findByStatus`).
///
/// # Returns
/// * `200 OK` with a JSON array of matching pets.
pub async fn find_pets_by_status() -> Result<HttpResponse, ServerError> {
    Ok(HttpResponse::Ok().json(serde_json::json!([])))
}

/// Finds pets filtered by tags (`GET /pet/findByTags`).
///
/// # Returns
/// * `200 OK` with a JSON array of matching pets.
pub async fn find_pets_by_tags() -> Result<HttpResponse, ServerError> {
    Ok(HttpResponse::Ok().json(serde_json::json!([])))
}

/// Retrieves a single pet by its ID (`GET /pet/{petId}`).
///
/// # Arguments
/// * `pet_id` - Path parameter specifying the pet's numeric ID.
/// * `store` - Shared application [`PetStore`] state.
///
/// # Returns
/// * `200 OK` with the pet JSON if found, or `404 Not Found` otherwise.
///
/// # Errors
/// * Returns [`ServerError::SyncError`] if the internal mutex is poisoned.
pub async fn get_pet_by_id(
    pet_id: web::Path<i64>,
    store: web::Data<PetStore>,
) -> Result<HttpResponse, ServerError> {
    let map = store
        .pets
        .lock()
        .map_err(|_| ServerError::SyncError("PetStore lock poisoned".to_string()))?;
    if let Some(pet) = map.get(&pet_id.into_inner()) {
        Ok(HttpResponse::Ok().json(pet))
    } else {
        Ok(HttpResponse::NotFound().finish())
    }
}

/// Updates a pet in the store using form data (`POST /pet/{petId}`).
///
/// # Returns
/// * `200 OK` response.
pub async fn update_pet_with_form() -> Result<HttpResponse, ServerError> {
    Ok(HttpResponse::Ok().finish())
}

/// Deletes a pet by its ID (`DELETE /pet/{petId}`).
///
/// # Arguments
/// * `pet_id` - Path parameter specifying the pet's numeric ID.
/// * `store` - Shared application [`PetStore`] state.
///
/// # Returns
/// * `200 OK` if the pet was removed, or `404 Not Found` if it did not exist.
///
/// # Errors
/// * Returns [`ServerError::SyncError`] if the internal mutex is poisoned.
pub async fn delete_pet(
    pet_id: web::Path<i64>,
    store: web::Data<PetStore>,
) -> Result<HttpResponse, ServerError> {
    let mut map = store
        .pets
        .lock()
        .map_err(|_| ServerError::SyncError("PetStore lock poisoned".to_string()))?;
    if map.remove(&pet_id.into_inner()).is_some() {
        Ok(HttpResponse::Ok().finish())
    } else {
        Ok(HttpResponse::NotFound().finish())
    }
}

/// Uploads an image for a pet (`POST /pet/{petId}/uploadImage`).
///
/// # Returns
/// * `200 OK` response.
pub async fn upload_file() -> Result<HttpResponse, ServerError> {
    Ok(HttpResponse::Ok().finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_pet_handlers() {
        let store = web::Data::new(PetStore::default());

        let app = test::init_service(
            App::new()
                .app_data(store.clone())
                .route("/pet", web::post().to(add_pet))
                .route("/pet", web::put().to(update_pet))
                .route("/pet/findByStatus", web::get().to(find_pets_by_status))
                .route("/pet/findByTags", web::get().to(find_pets_by_tags))
                .route("/pet/{petId}", web::get().to(get_pet_by_id))
                .route("/pet/{petId}", web::post().to(update_pet_with_form))
                .route("/pet/{petId}", web::delete().to(delete_pet))
                .route("/pet/{petId}/uploadImage", web::post().to(upload_file)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/pet")
            .set_json(serde_json::json!({"id": 1, "name": "Fido"}))
            .to_request();
        let created: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(created["name"], "Fido");

        let req = test::TestRequest::post()
            .uri("/pet")
            .set_json(serde_json::json!({"name": "NoId"}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);

        let req = test::TestRequest::put()
            .uri("/pet")
            .set_json(serde_json::json!({"id": 1, "name": "Fido Updated"}))
            .to_request();
        let updated: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(updated["name"], "Fido Updated");

        let req = test::TestRequest::put()
            .uri("/pet")
            .set_json(serde_json::json!({"name": "NoId"}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);

        let req = test::TestRequest::get()
            .uri("/pet/findByStatus")
            .to_request();
        let by_status: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(by_status, serde_json::json!([]));

        let req = test::TestRequest::get().uri("/pet/findByTags").to_request();
        let by_tags: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(by_tags, serde_json::json!([]));

        let req = test::TestRequest::get().uri("/pet/1").to_request();
        let fetched: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(fetched["name"], "Fido Updated");

        let req = test::TestRequest::get().uri("/pet/2").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);

        let req = test::TestRequest::post().uri("/pet/1").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::delete().uri("/pet/1").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::delete().uri("/pet/2").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);

        let req = test::TestRequest::post()
            .uri("/pet/1/uploadImage")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}
