#![deny(missing_docs)]

//! Pet resource HTTP handlers backed by thread-safe in-memory state.

use crate::error::ServerError;
use crate::models::handlers::pet::{
    DeletePetHeaders, FindPetsByStatusQuery, FindPetsByTagsQuery, UpdatePetWithFormForm,
    UploadFileQuery,
};
use actix_web::{web, HttpRequest, HttpResponse};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// Thread-safe in-memory store for `Pet` JSON payloads keyed by integer ID.
#[derive(Default)]
pub struct PetStore {
    /// Map of pet ID to raw JSON object.
    pub pets: Mutex<HashMap<i64, Value>>,
}

/// Adds a new pet to the store (`POST /pet`).
///
/// # Arguments
/// * `body` - JSON payload representing the pet, containing an integer `"id"`.
/// * `store` - Shared application [`PetStore`] state.
///
/// # Returns
/// * `200 OK` with the created pet JSON, or `400 Bad Request` if `"id"` is missing.
///
/// # Errors
/// * Returns [`ServerError`] wrapping [`cdd_core::error::AppError::SyncError`] if the internal mutex is poisoned.
pub async fn add_pet(
    body: web::Json<Value>,
    store: web::Data<PetStore>,
) -> Result<HttpResponse, ServerError> {
    let mut map = store
        .pets
        .lock()
        .map_err(|_| ServerError::sync_error("PetStore lock poisoned"))?;
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
/// * Returns [`ServerError`] wrapping [`cdd_core::error::AppError::SyncError`] if the internal mutex is poisoned.
pub async fn update_pet(
    body: web::Json<Value>,
    store: web::Data<PetStore>,
) -> Result<HttpResponse, ServerError> {
    let mut map = store
        .pets
        .lock()
        .map_err(|_| ServerError::sync_error("PetStore lock poisoned"))?;
    if let Some(id) = body.get("id").and_then(|v| v.as_i64()) {
        map.insert(id, body.clone());
        Ok(HttpResponse::Ok().json(body.into_inner()))
    } else {
        Ok(HttpResponse::BadRequest().finish())
    }
}

/// Finds pets filtered by status (`GET /pet/findByStatus`).
///
/// # Arguments
/// * `query` - Optional [`FindPetsByStatusQuery`] filter parameters.
/// * `store` - Optional shared [`PetStore`] state.
///
/// # Returns
/// * `200 OK` with a JSON array of matching pets.
///
/// # Errors
/// * Returns [`ServerError`] wrapping [`cdd_core::error::AppError::SyncError`] if the internal mutex is poisoned.
pub async fn find_pets_by_status(
    query: Option<web::Query<FindPetsByStatusQuery>>,
    store: Option<web::Data<PetStore>>,
) -> Result<HttpResponse, ServerError> {
    let Some(store) = store else {
        return Ok(HttpResponse::Ok().json(serde_json::json!([])));
    };
    let map = store
        .pets
        .lock()
        .map_err(|_| ServerError::sync_error("PetStore lock poisoned"))?;
    let statuses = query.map(|q| q.into_inner().status).unwrap_or_default();
    let filtered: Vec<Value> = map
        .values()
        .filter(|pet| {
            statuses.is_empty()
                || pet
                    .get("status")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| statuses.iter().any(|wanted| wanted == s))
        })
        .cloned()
        .collect();
    Ok(HttpResponse::Ok().json(filtered))
}

/// Finds pets filtered by tags (`GET /pet/findByTags`).
///
/// # Arguments
/// * `query` - Optional [`FindPetsByTagsQuery`] filter parameters.
/// * `store` - Optional shared [`PetStore`] state.
///
/// # Returns
/// * `200 OK` with a JSON array of matching pets.
///
/// # Errors
/// * Returns [`ServerError`] wrapping [`cdd_core::error::AppError::SyncError`] if the internal mutex is poisoned.
pub async fn find_pets_by_tags(
    query: Option<web::Query<FindPetsByTagsQuery>>,
    store: Option<web::Data<PetStore>>,
) -> Result<HttpResponse, ServerError> {
    let Some(store) = store else {
        return Ok(HttpResponse::Ok().json(serde_json::json!([])));
    };
    let map = store
        .pets
        .lock()
        .map_err(|_| ServerError::sync_error("PetStore lock poisoned"))?;
    let wanted_tags = query.map(|q| q.into_inner().tags).unwrap_or_default();
    let filtered: Vec<Value> = map
        .values()
        .filter(|pet| {
            wanted_tags.is_empty()
                || pet
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .is_some_and(|arr| {
                        arr.iter().any(|t| {
                            t.as_str()
                                .or_else(|| t.get("name").and_then(|n| n.as_str()))
                                .is_some_and(|name| wanted_tags.iter().any(|w| w == name))
                        })
                    })
        })
        .cloned()
        .collect();
    Ok(HttpResponse::Ok().json(filtered))
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
/// * Returns [`ServerError`] wrapping [`cdd_core::error::AppError::SyncError`] if the internal mutex is poisoned.
pub async fn get_pet_by_id(
    pet_id: web::Path<i64>,
    store: web::Data<PetStore>,
) -> Result<HttpResponse, ServerError> {
    let map = store
        .pets
        .lock()
        .map_err(|_| ServerError::sync_error("PetStore lock poisoned"))?;
    if let Some(pet) = map.get(&pet_id.into_inner()) {
        Ok(HttpResponse::Ok().json(pet))
    } else {
        Ok(HttpResponse::NotFound().finish())
    }
}

/// Updates a pet in the store using form data (`POST /pet/{petId}`).
///
/// # Arguments
/// * `pet_id` - Optional path parameter specifying the pet's numeric ID.
/// * `form` - Optional [`UpdatePetWithFormForm`] form payload.
/// * `store` - Optional shared [`PetStore`] state.
///
/// # Returns
/// * `200 OK` response.
///
/// # Errors
/// * Returns [`ServerError`] wrapping [`cdd_core::error::AppError::SyncError`] if the internal mutex is poisoned.
pub async fn update_pet_with_form(
    pet_id: Option<web::Path<i64>>,
    form: Option<web::Form<UpdatePetWithFormForm>>,
    store: Option<web::Data<PetStore>>,
) -> Result<HttpResponse, ServerError> {
    if let (Some(id), Some(form_data), Some(store_data)) = (pet_id, form, store) {
        let mut map = store_data
            .pets
            .lock()
            .map_err(|_| ServerError::sync_error("PetStore lock poisoned"))?;
        if let Some(Value::Object(pet_obj)) = map.get_mut(&id.into_inner()) {
            let f = form_data.into_inner();
            if let Some(name) = f.name {
                pet_obj.insert("name".to_string(), Value::String(name));
            }
            if let Some(status) = f.status {
                pet_obj.insert("status".to_string(), Value::String(status));
            }
        }
    }
    Ok(HttpResponse::Ok().finish())
}

/// Deletes a pet by its ID (`DELETE /pet/{petId}`).
///
/// # Arguments
/// * `req` - Incoming HTTP request (inspected for optional `api_key` header).
/// * `pet_id` - Path parameter specifying the pet's numeric ID.
/// * `store` - Shared application [`PetStore`] state.
///
/// # Returns
/// * `200 OK` if the pet was removed, or `404 Not Found` if it did not exist.
///
/// # Errors
/// * Returns [`ServerError`] wrapping [`cdd_core::error::AppError::SyncError`] if the internal mutex is poisoned.
pub async fn delete_pet(
    req: HttpRequest,
    pet_id: web::Path<i64>,
    store: web::Data<PetStore>,
) -> Result<HttpResponse, ServerError> {
    let hdrs = DeletePetHeaders {
        api_key: req
            .headers()
            .get("api_key")
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned),
    };
    let _ = &hdrs.api_key;
    let mut map = store
        .pets
        .lock()
        .map_err(|_| ServerError::sync_error("PetStore lock poisoned"))?;
    if map.remove(&pet_id.into_inner()).is_some() {
        Ok(HttpResponse::Ok().finish())
    } else {
        Ok(HttpResponse::NotFound().finish())
    }
}

/// Uploads an image for a pet (`POST /pet/{petId}/uploadImage`).
///
/// # Arguments
/// * `query` - Optional [`UploadFileQuery`] metadata query parameter.
///
/// # Returns
/// * `200 OK` response with upload metadata JSON.
pub async fn upload_file(
    query: Option<web::Query<UploadFileQuery>>,
) -> Result<HttpResponse, ServerError> {
    let meta = query
        .and_then(|q| q.into_inner().additional_metadata)
        .unwrap_or_default();
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "code": 200,
        "type": "uploaded",
        "message": meta
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_pet_crud_lifecycle_and_filtering() {
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

        // Add pet with status & tags
        let pet_json = serde_json::json!({
            "id": 10,
            "name": "Doggie",
            "status": "available",
            "tags": ["friendly"]
        });
        let req = test::TestRequest::post()
            .uri("/pet")
            .set_json(&pet_json)
            .to_request();
        let resp: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp["name"], "Doggie");

        // Bad request when id is missing
        let req = test::TestRequest::post()
            .uri("/pet")
            .set_json(serde_json::json!({"name": "NoId"}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);

        // Update pet
        let updated_json = serde_json::json!({
            "id": 10,
            "name": "SuperDoggie",
            "status": "sold",
            "tags": [{"name": "hero"}]
        });
        let req = test::TestRequest::put()
            .uri("/pet")
            .set_json(&updated_json)
            .to_request();
        let resp: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp["name"], "SuperDoggie");

        let req = test::TestRequest::put()
            .uri("/pet")
            .set_json(serde_json::json!({"name": "NoId"}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);

        // Get by ID
        let req = test::TestRequest::get().uri("/pet/10").to_request();
        let resp: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp["name"], "SuperDoggie");

        let req = test::TestRequest::get().uri("/pet/999").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);

        // Filter by status & tags via HTTP and direct typed Query
        let req = test::TestRequest::get()
            .uri("/pet/findByStatus")
            .to_request();
        let by_status: Vec<Value> = test::call_and_read_body_json(&app, req).await;
        assert_eq!(by_status.len(), 1);

        let direct_status = find_pets_by_status(
            Some(web::Query(FindPetsByStatusQuery {
                status: vec!["sold".to_string()],
            })),
            Some(store.clone()),
        )
        .await
        .expect("direct status");
        assert!(direct_status.status().is_success());

        let req = test::TestRequest::get().uri("/pet/findByTags").to_request();
        let by_tags: Vec<Value> = test::call_and_read_body_json(&app, req).await;
        assert_eq!(by_tags.len(), 1);

        // Also add a pet with string tags to cover both tag representations
        store.pets.lock().expect("lock").insert(
            11,
            serde_json::json!({"id": 11, "name": "Cat", "status": "available", "tags": ["feline"]}),
        );
        let direct_tags = find_pets_by_tags(
            Some(web::Query(FindPetsByTagsQuery {
                tags: vec!["hero".to_string(), "feline".to_string()],
            })),
            Some(store.clone()),
        )
        .await
        .expect("direct tags");
        assert!(direct_tags.status().is_success());
        store.pets.lock().expect("lock").remove(&11);

        // Update with form data
        let req = test::TestRequest::post()
            .uri("/pet/10")
            .set_form(UpdatePetWithFormForm {
                name: Some("FormDoggie".to_string()),
                status: Some("pending".to_string()),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post()
            .uri("/pet/10")
            .set_form(UpdatePetWithFormForm {
                name: None,
                status: None,
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post()
            .uri("/pet/999")
            .set_form(UpdatePetWithFormForm {
                name: Some("Ghost".to_string()),
                status: None,
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Upload image with metadata
        let req = test::TestRequest::post()
            .uri("/pet/10/uploadImage?additional_metadata=vaccinated")
            .to_request();
        let upload_resp: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(upload_resp["message"], "vaccinated");

        let req = test::TestRequest::post()
            .uri("/pet/10/uploadImage")
            .to_request();
        let upload_empty: Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(upload_empty["message"], "");

        // Delete pet with api_key header
        let req = test::TestRequest::delete()
            .uri("/pet/10")
            .insert_header(("api_key", "special-key"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::delete().uri("/pet/10").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);

        // Bare app without PetStore Data
        let bare_app = test::init_service(
            App::new()
                .route("/pet/findByStatus", web::get().to(find_pets_by_status))
                .route("/pet/findByTags", web::get().to(find_pets_by_tags))
                .route("/pet/{petId}", web::post().to(update_pet_with_form)),
        )
        .await;
        let r1 = test::TestRequest::get()
            .uri("/pet/findByStatus")
            .to_request();
        assert!(test::call_service(&bare_app, r1)
            .await
            .status()
            .is_success());
        let r2 = test::TestRequest::get().uri("/pet/findByTags").to_request();
        assert!(test::call_service(&bare_app, r2)
            .await
            .status()
            .is_success());
        let r3 = test::TestRequest::post().uri("/pet/10").to_request();
        assert!(test::call_service(&bare_app, r3)
            .await
            .status()
            .is_success());
    }

    #[actix_web::test]
    async fn test_pet_store_poisoned_lock() {
        let store = web::Data::new(PetStore::default());
        let store_clone = store.clone();
        let _ = std::thread::spawn(move || {
            let _guard = store_clone.pets.lock().expect("lock");
            panic!("poison lock");
        })
        .join();

        let app = test::init_service(
            App::new()
                .app_data(store)
                .route("/pet", web::post().to(add_pet))
                .route("/pet", web::put().to(update_pet))
                .route("/pet/findByStatus", web::get().to(find_pets_by_status))
                .route("/pet/findByTags", web::get().to(find_pets_by_tags))
                .route("/pet/{petId}", web::get().to(get_pet_by_id))
                .route("/pet/{petId}", web::post().to(update_pet_with_form))
                .route("/pet/{petId}", web::delete().to(delete_pet)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/pet")
            .set_json(serde_json::json!({"id": 1}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        let req = test::TestRequest::put()
            .uri("/pet")
            .set_json(serde_json::json!({"id": 1}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        let req = test::TestRequest::get()
            .uri("/pet/findByStatus")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        let req = test::TestRequest::get().uri("/pet/findByTags").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        let req = test::TestRequest::get().uri("/pet/1").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        let req = test::TestRequest::post()
            .uri("/pet/1")
            .set_form(UpdatePetWithFormForm {
                name: Some("x".to_string()),
                status: None,
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );

        let req = test::TestRequest::delete().uri("/pet/1").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
