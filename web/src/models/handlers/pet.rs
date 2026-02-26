use actix_web::{web, HttpResponse, Responder};
use actix_multipart::Multipart;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
use chrono::{DateTime, Utc, NaiveDate, NaiveDateTime};

/// Query parameters for `add_pet`.
#[derive(Debug, Clone, Deserialize)]
pub struct AddPetQuery {
    /// Pet object that needs to be added to the store
    pub body: Pet,
}

/// Add a new pet to the store
///
pub async fn add_pet(query: web::Query<AddPetQuery>, petstore_auth: web::ReqData<security::PetstoreAuth<(security::scopes::WritePets, security::scopes::ReadPets)>>) -> impl Responder {
    todo!()
}

/// Query parameters for `update_pet`.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePetQuery {
    /// Pet object that needs to be added to the store
    pub body: Pet,
}

/// Update an existing pet
///
pub async fn update_pet(query: web::Query<UpdatePetQuery>, petstore_auth: web::ReqData<security::PetstoreAuth<(security::scopes::WritePets, security::scopes::ReadPets)>>) -> impl Responder {
    todo!()
}

/// Query parameters for `find_pets_by_status`.
#[derive(Debug, Clone, Deserialize)]
pub struct FindPetsByStatusQuery {
    /// Status values that need to be considered for filter
    pub status: Vec<String>,
}

/// Finds Pets by status
///
/// Multiple status values can be provided with comma separated strings
pub async fn find_pets_by_status(query: web::Query<FindPetsByStatusQuery>, petstore_auth: web::ReqData<security::PetstoreAuth<(security::scopes::WritePets, security::scopes::ReadPets)>>) -> impl Responder {
    todo!()
}

/// Query parameters for `find_pets_by_tags`.
#[derive(Debug, Clone, Deserialize)]
pub struct FindPetsByTagsQuery {
    /// Tags to filter by
    pub tags: Vec<String>,
}

/// Finds Pets by tags
///
/// Multiple tags can be provided with comma separated strings. Use tag1, tag2, tag3 for testing.
#[deprecated]
pub async fn find_pets_by_tags(query: web::Query<FindPetsByTagsQuery>, petstore_auth: web::ReqData<security::PetstoreAuth<(security::scopes::WritePets, security::scopes::ReadPets)>>) -> impl Responder {
    todo!()
}

/// Find pet by ID
///
/// Returns a single pet
pub async fn get_pet_by_id(pet_id: web::Path<i64>, api_key: web::ReqData<security::ApiKey>) -> impl Responder {
    todo!()
}

/// Query parameters for `update_pet_with_form`.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePetWithFormQuery {
    /// Updated name of the pet
    pub name: Option<String>,
    /// Updated status of the pet
    pub status: Option<String>,
}

/// Updates a pet in the store with form data
///
pub async fn update_pet_with_form(pet_id: web::Path<i64>, query: web::Query<UpdatePetWithFormQuery>, petstore_auth: web::ReqData<security::PetstoreAuth<(security::scopes::WritePets, security::scopes::ReadPets)>>) -> impl Responder {
    todo!()
}

/// Deletes a pet
///
pub async fn delete_pet(pet_id: web::Path<i64>, api_key: web::Header<String>, petstore_auth: web::ReqData<security::PetstoreAuth<(security::scopes::WritePets, security::scopes::ReadPets)>>) -> impl Responder {
    todo!()
}

/// Query parameters for `upload_file`.
#[derive(Debug, Clone, Deserialize)]
pub struct UploadFileQuery {
    /// Additional data to pass to server
    #[serde(rename = "additionalMetadata")]
    pub additionalmetadata: Option<String>,
    /// file to upload
    pub file: Option<Vec<u8>>,
}

/// uploads an image
///
pub async fn upload_file(pet_id: web::Path<i64>, query: web::Query<UploadFileQuery>, petstore_auth: web::ReqData<security::PetstoreAuth<(security::scopes::WritePets, security::scopes::ReadPets)>>) -> impl Responder {
    todo!()
}

