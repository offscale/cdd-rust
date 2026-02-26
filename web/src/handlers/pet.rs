use actix_web::{web, HttpResponse, Responder};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

// We use an App state with a Mutex<HashMap> for storing pets
pub struct PetStore {
    pub pets: Mutex<HashMap<i64, Value>>,
}

pub async fn add_pet(body: web::Json<Value>, store: web::Data<PetStore>) -> impl Responder {
    let mut map = store.pets.lock().unwrap();
    if let Some(id) = body.get("id").and_then(|v| v.as_i64()) {
        map.insert(id, body.clone());
        HttpResponse::Ok().json(body.into_inner())
    } else {
        HttpResponse::BadRequest().finish()
    }
}

pub async fn update_pet(body: web::Json<Value>, store: web::Data<PetStore>) -> impl Responder {
    let mut map = store.pets.lock().unwrap();
    if let Some(id) = body.get("id").and_then(|v| v.as_i64()) {
        map.insert(id, body.clone());
        HttpResponse::Ok().json(body.into_inner())
    } else {
        HttpResponse::BadRequest().finish()
    }
}

pub async fn find_pets_by_status() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!([]))
}

pub async fn find_pets_by_tags() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!([]))
}

pub async fn get_pet_by_id(pet_id: web::Path<i64>, store: web::Data<PetStore>) -> impl Responder {
    let map = store.pets.lock().unwrap();
    if let Some(pet) = map.get(&pet_id.into_inner()) {
        HttpResponse::Ok().json(pet)
    } else {
        HttpResponse::NotFound().finish()
    }
}

pub async fn update_pet_with_form() -> impl Responder {
    HttpResponse::Ok().finish()
}

pub async fn delete_pet(pet_id: web::Path<i64>, store: web::Data<PetStore>) -> impl Responder {
    let mut map = store.pets.lock().unwrap();
    if map.remove(&pet_id.into_inner()).is_some() {
        HttpResponse::Ok().finish()
    } else {
        HttpResponse::NotFound().finish()
    }
}

pub async fn upload_file() -> impl Responder {
    HttpResponse::Ok().finish()
}
