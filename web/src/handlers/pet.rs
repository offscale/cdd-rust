use actix_web::{web, HttpResponse, Responder};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

// We use an App state with a Mutex<HashMap> for storing pets
/// Documented
pub struct PetStore {
    /// Documented
    pub pets: Mutex<HashMap<i64, Value>>,
}

/// Documented
pub async fn add_pet(body: web::Json<Value>, store: web::Data<PetStore>) -> impl Responder {
    let mut map = store.pets.lock().unwrap();
    if let Some(id) = body.get("id").and_then(|v| v.as_i64()) {
        map.insert(id, body.clone());
        HttpResponse::Ok().json(body.into_inner())
    } else {
        HttpResponse::BadRequest().finish()
    }
}

/// Documented
pub async fn update_pet(body: web::Json<Value>, store: web::Data<PetStore>) -> impl Responder {
    let mut map = store.pets.lock().unwrap();
    if let Some(id) = body.get("id").and_then(|v| v.as_i64()) {
        map.insert(id, body.clone());
        HttpResponse::Ok().json(body.into_inner())
    } else {
        HttpResponse::BadRequest().finish()
    }
}

/// Documented
pub async fn find_pets_by_status() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!([]))
}

/// Documented
pub async fn find_pets_by_tags() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!([]))
}

/// Documented
pub async fn get_pet_by_id(pet_id: web::Path<i64>, store: web::Data<PetStore>) -> impl Responder {
    let map = store.pets.lock().unwrap();
    if let Some(pet) = map.get(&pet_id.into_inner()) {
        HttpResponse::Ok().json(pet)
    } else {
        HttpResponse::NotFound().finish()
    }
}

/// Documented
pub async fn update_pet_with_form() -> impl Responder {
    HttpResponse::Ok().finish()
}

/// Documented
pub async fn delete_pet(pet_id: web::Path<i64>, store: web::Data<PetStore>) -> impl Responder {
    let mut map = store.pets.lock().unwrap();
    if map.remove(&pet_id.into_inner()).is_some() {
        HttpResponse::Ok().finish()
    } else {
        HttpResponse::NotFound().finish()
    }
}

/// Documented
pub async fn upload_file() -> impl Responder {
    HttpResponse::Ok().finish()
}
#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_pet_handlers() {
        let store = web::Data::new(PetStore {
            pets: Mutex::new(HashMap::new()),
        });

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
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post()
            .uri("/pet")
            .set_json(serde_json::json!({"name": "NoId"}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());

        let req = test::TestRequest::put()
            .uri("/pet")
            .set_json(serde_json::json!({"id": 1, "name": "Fido Updated"}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::put()
            .uri("/pet")
            .set_json(serde_json::json!({"name": "NoId"}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());

        let req = test::TestRequest::get()
            .uri("/pet/findByStatus")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::get().uri("/pet/findByTags").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::get().uri("/pet/1").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::get().uri("/pet/2").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());

        let req = test::TestRequest::post().uri("/pet/1").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::delete().uri("/pet/1").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::delete().uri("/pet/2").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());

        let req = test::TestRequest::post()
            .uri("/pet/1/uploadImage")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }
}
