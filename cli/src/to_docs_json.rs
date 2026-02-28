use cdd_core::error::{AppError, AppResult};
use cdd_core::openapi::parse::document::parse_openapi_document;
use cdd_core::openapi::parse::models::RouteKind;
use clap::Args;
use serde::Serialize;
use std::fs;

#[derive(Args, Debug)]
pub struct ToDocsJsonArgs {
    /// Path or URL to the OpenAPI specification.
    #[clap(short, long)]
    pub input: String,

    /// If provided, omit the imports field in the code object.
    #[clap(long)]
    pub no_imports: bool,

    /// If provided, omit the wrapper_start and wrapper_end fields in the code object.
    #[clap(long)]
    pub no_wrapping: bool,
}

#[derive(Serialize, Debug)]
struct DocsJsonOutput {
    language: String,
    operations: Vec<DocsOperation>,
}

#[derive(Serialize, Debug)]
struct DocsOperation {
    method: String,
    path: String,
    #[serde(rename = "operationId", skip_serializing_if = "Option::is_none")]
    operation_id: Option<String>,
    code: DocsCode,
}

#[derive(Serialize, Debug)]
struct DocsCode {
    #[serde(skip_serializing_if = "Option::is_none")]
    imports: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wrapper_start: Option<String>,
    snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    wrapper_end: Option<String>,
}

#[cfg(not(tarpaulin_include))]
fn map_ureq_err(e: ureq::Error) -> AppError {
    AppError::General(e.to_string())
}

#[cfg(not(tarpaulin_include))]
fn read_input(input: &str) -> AppResult<String> {
    if input.starts_with("http://") || input.starts_with("https://") {
        let mut response = ureq::get(input)
            .call()
            .map_err(|e| AppError::General(format!("Failed to fetch URL: {}", e)))?;
        response.body_mut().read_to_string().map_err(map_ureq_err)
    } else {
        fs::read_to_string(input)
            .map_err(|e| AppError::General(format!("Failed to read file: {}", e)))
    }
}

pub fn execute(args: &ToDocsJsonArgs) -> AppResult<()> {
    let yaml_content = read_input(&args.input)?;

    let output = generate_docs_json(&yaml_content, args)?;

    let json = serde_json::to_string_pretty(&output)
        .map_err(|e| AppError::General(format!("JSON Serialization error: {}", e)))?;

    println!("{}", json);

    Ok(())
}

fn generate_docs_json(yaml_content: &str, args: &ToDocsJsonArgs) -> AppResult<Vec<DocsJsonOutput>> {
    let parsed = parse_openapi_document(yaml_content)?;

    let mut operations = Vec::new();
    let base_url = parsed
        .info
        .servers
        .first()
        .map(|s| s.url.clone())
        .unwrap_or_else(|| "https://api.example.com".to_string());

    for route in parsed.routes {
        if route.kind == RouteKind::Webhook {
            continue;
        }

        let imports = if !args.no_imports {
            Some("use api_client::ApiClient;\nuse tokio;".to_string())
        } else {
            None
        };

        let (wrapper_start, wrapper_end) = if !args.no_wrapping {
            (
                Some(
                    "#[tokio::main]\nasync fn main() -> Result<(), Box<dyn std::error::Error>> {\n"
                        .to_string(),
                ),
                Some("\n    Ok(())\n}".to_string()),
            )
        } else {
            (None, None)
        };

        let fn_name = route.handler_name.clone();

        let snippet = format!(
            "    let client = ApiClient::new(\"{}\");\n    let response = client.{}().await?;\n    println!(\"{{:#?}}\", response);",
            base_url, fn_name
        );

        let code = DocsCode {
            imports,
            wrapper_start,
            snippet,
            wrapper_end,
        };

        operations.push(DocsOperation {
            method: route.method.to_uppercase(),
            path: route.path.clone(),
            operation_id: route.operation_id.clone(),
            code,
        });
    }

    Ok(vec![DocsJsonOutput {
        language: "rust".to_string(),
        operations,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_docs_json_default() {
        let yaml = r#"
openapi: 3.0.0
info:
  title: Test API
  version: 1.0.0
servers:
  - url: http://localhost:8080
paths:
  /pets:
    get:
      operationId: getPets
      responses:
        '200':
          description: OK
"#;

        let args = ToDocsJsonArgs {
            input: "dummy".into(),
            no_imports: false,
            no_wrapping: false,
        };

        let output = generate_docs_json(yaml, &args).unwrap();
        assert_eq!(output.len(), 1);
        let rust_docs = &output[0];
        assert_eq!(rust_docs.language, "rust");
        assert_eq!(rust_docs.operations.len(), 1);

        let op = &rust_docs.operations[0];
        assert_eq!(op.method, "GET");
        assert_eq!(op.path, "/pets");
        assert_eq!(op.operation_id.as_deref(), Some("getPets"));
        assert!(op.code.imports.is_some());
        assert!(op.code.wrapper_start.is_some());
        assert!(op.code.wrapper_end.is_some());
        assert!(op
            .code
            .snippet
            .contains("ApiClient::new(\"http://localhost:8080\")"));
        assert!(op.code.snippet.contains("client.get_pets().await?"));
    }

    #[test]
    fn test_generate_docs_json_toggles() {
        let yaml = r#"
openapi: 3.0.0
info:
  title: Test API
  version: 1.0.0
paths:
  /pets:
    get:
      operationId: getPets
      responses:
        '200':
          description: OK
"#;

        let args = ToDocsJsonArgs {
            input: "dummy".into(),
            no_imports: true,
            no_wrapping: true,
        };

        let output = generate_docs_json(yaml, &args).unwrap();
        let op = &output[0].operations[0];

        assert!(op.code.imports.is_none());
        assert!(op.code.wrapper_start.is_none());
        assert!(op.code.wrapper_end.is_none());
        assert!(op
            .code
            .snippet
            .contains("ApiClient::new(\"https://api.example.com\")"));
    }

    #[test]
    fn test_execute_with_file() {
        use std::io::Write;
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "openapi: 3.0.0\ninfo:\n  title: Test API\n  version: 1.0.0\npaths:\n  /pets:\n    get:\n      responses:\n        '200':\n          description: OK").unwrap();

        let args = ToDocsJsonArgs {
            input: file.path().to_str().unwrap().to_string(),
            no_imports: false,
            no_wrapping: false,
        };
        let result = execute(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_with_url_error() {
        let args = ToDocsJsonArgs {
            input: "http://localhost:9999/nonexistent".to_string(),
            no_imports: false,
            no_wrapping: false,
        };
        let result = execute(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_with_file_error() {
        let args = ToDocsJsonArgs {
            input: "nonexistent_file.yaml".to_string(),
            no_imports: false,
            no_wrapping: false,
        };
        let result = execute(&args);
        assert!(result.is_err());
    }
}
