use cdd_rust::from_openapi::generate;
use pretty_assertions::assert_eq;
use std::fs;

#[test]
fn test_generate_from_openapi() {
    let openapi_spec = r#"
openapi: 3.0.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    TestSchema:
      type: object
      properties:
        id:
          type: integer
          format: int64
        name:
          type: string
    "#;

    let expected_rust_code = r#"#[derive(
    Clone,
    Debug,
    PartialEq,
    Queryable,
    Insertable,
    serde::Deserialize,
    serde::Serialize,
)]
#[diesel(table_name = test_schema)]
pub struct TestSchema {
    pub id: i64,
    pub name: String,
}
"#;

    let expected_schema_code = r#"diesel :: table ! { test_schema (id) { id -> BigInt , name -> Text , } } "#;

    let input_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();
    let schema_output_dir = tempfile::tempdir().unwrap();

    let spec_path = input_dir.path().join("openapi.yml");
    fs::write(&spec_path, openapi_spec).unwrap();

    generate(
        &spec_path,
        &output_dir.path().to_path_buf(),
        &schema_output_dir.path().to_path_buf(),
    )
    .unwrap();

    let generated_file_path = output_dir.path().join("models.rs");
    let generated_rust_code = fs::read_to_string(generated_file_path).unwrap();

    assert_eq!(generated_rust_code, expected_rust_code);

    let generated_schema_file_path = schema_output_dir.path().join("schema.rs");
    let generated_schema_code = fs::read_to_string(generated_schema_file_path).unwrap();

    assert_eq!(
        generated_schema_code.trim(),
        expected_schema_code.trim()
    );
}

#[test]
fn test_generate_routes_from_openapi() {
    let openapi_spec = r#"
openapi: 3.0.0
info:
  title: Test API
  version: 1.0.0
paths:
  /test:
    get:
      operationId: test_route
      responses:
        '200':
          description: OK
    "#;

    let expected_rust_code = r#"#[get("/test")]
async fn test_route() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}
"#;

    let input_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();

    let spec_path = input_dir.path().join("openapi.yml");
    fs::write(&spec_path, openapi_spec).unwrap();

    cdd_rust::from_openapi::generate_routes(&spec_path, &output_dir.path().to_path_buf()).unwrap();

    let generated_file_path = output_dir.path().join("routes.rs");
    let generated_rust_code = fs::read_to_string(generated_file_path).unwrap();

    assert_eq!(generated_rust_code, expected_rust_code);
}

#[test]
fn test_generate_tests_from_openapi() {
    let openapi_spec = r#"
openapi: 3.0.0
info:
  title: Test API
  version: 1.0.0
paths:
  /test:
    get:
      operationId: test_route
      responses:
        '200':
          description: OK
    "#;

    let expected_rust_code = r#"#[actix_web::test]
async fn test_test_route() {
    let req = actix_web::test::TestRequest::get().uri("/test").to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}
"#;

    let input_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();

    let spec_path = input_dir.path().join("openapi.yml");
    fs::write(&spec_path, openapi_spec).unwrap();

    cdd_rust::from_openapi::generate_tests(&spec_path, &output_dir.path().to_path_buf()).unwrap();

    let generated_file_path = output_dir.path().join("tests.rs");
    let generated_rust_code = fs::read_to_string(generated_file_path).unwrap();

    assert_eq!(generated_rust_code.trim(), expected_rust_code.trim());
}
