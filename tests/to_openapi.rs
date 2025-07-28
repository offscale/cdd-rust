use cdd_rust::to_openapi::generate;
use pretty_assertions::assert_eq;
use std::fs;

#[test]
fn test_generate_to_openapi() {
    let rust_code = r#"
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct NestedSchema {
    pub id: i64,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TestSchema {
    pub id: i64,
    pub name: String,
    pub value: f64,
    pub is_valid: bool,
    pub nested: NestedSchema,
}
    "#;

    let expected_openapi_spec = r#"
openapi: 3.0.0
info:
  title: Test API
  version: 1.0.0
paths: {}
components:
  schemas:
    NestedSchema:
      type: object
      properties:
        id:
          type: integer
          format: int64
    TestSchema:
      type: object
      properties:
        id:
          type: integer
          format: int64
        is_valid:
          type: boolean
        name:
          type: string
        nested:
          $ref: '#/components/schemas/NestedSchema'
        value:
          type: number
    "#;

    let input_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();

    let rust_path = input_dir.path().join("models.rs");
    fs::write(&rust_path, rust_code).unwrap();

    let spec_path = output_dir.path().join("openapi.yml");
    generate(&rust_path, &spec_path).unwrap();

    let generated_openapi_spec = fs::read_to_string(spec_path).unwrap();

    assert_eq!(generated_openapi_spec.trim(), expected_openapi_spec.trim());
}

#[test]
fn test_generate_from_models() {
    let rust_code = r#"
#[derive(Clone, Debug, PartialEq, Queryable, Insertable, serde::Deserialize, serde::Serialize)]
#[diesel(table_name = "test_schema")]
pub struct TestSchema {
    pub id: i64,
    pub name: String,
}
    "#;

    let expected_openapi_spec = r#"
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

    let input_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();

    let rust_path = input_dir.path().join("models.rs");
    fs::write(&rust_path, rust_code).unwrap();

    let spec_path = output_dir.path().join("openapi.yml");
    cdd_rust::to_openapi::generate_from_models(input_dir.path(), &spec_path).unwrap();

    let generated_openapi_spec = fs::read_to_string(spec_path).unwrap();

    assert_eq!(generated_openapi_spec.trim(), expected_openapi_spec.trim());
}
