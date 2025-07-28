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

    let expected_rust_code = r#"#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TestSchema {
    pub id: i64,
    pub name: String,
}
"#;

    let input_dir = tempfile::tempdir().unwrap();
    let output_dir = tempfile::tempdir().unwrap();

    let spec_path = input_dir.path().join("openapi.yml");
    fs::write(&spec_path, openapi_spec).unwrap();

    generate(&spec_path, &output_dir.path().to_path_buf()).unwrap();

    let generated_file_path = output_dir.path().join("models.rs");
    let generated_rust_code = fs::read_to_string(generated_file_path).unwrap();

    assert_eq!(generated_rust_code, expected_rust_code);
}
