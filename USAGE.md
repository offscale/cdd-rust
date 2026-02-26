# cdd-rust CLI Usage

## 1. Sync Pipeline (DB ➔ Models)

Synchronize Diesel models and inject OpenAPI attributes.

```bash
cargo run -p cdd-cli -- sync 
  --schema-path web/src/schema.rs 
  --model-dir web/src/models
```

This performs the following:

1. Reads `schema.rs`.
2. Generates rust structs in `models/` using `diesel/dsync` logic.
3. **Patches** the files to add `use utoipa::ToSchema;` and derive macros.

*(Note: The generator strictly respects your `#![deny(missing_docs)]` constraints without injecting artificial `allow(missing_docs)` exceptions, encouraging proper model documentation).*

## 2. Scaffold (OpenAPI ➔ Handlers)

Scaffold endpoints directly from an OpenAPI file.

```bash
cargo run -p cdd-cli -- scaffold 
  --openapi-path docs/openapi.yaml 
  --output-dir web/src/handlers 
  --app-factory crate::create_app
```

This performs the following:

1. Parses the OpenAPI operations.
2. Generates missing Actix handlers and payloads with `todo!()` macros.
3. Safely injects Actix `.service()` registrations into the app factory.

## 3. Test Generation (OpenAPI ➔ Tests)

Scaffold integration tests to verify your implementation meets the contract.

```bash
cargo run -p cdd-cli -- test-gen 
  --openapi-path docs/openapi.yaml 
  --output-path web/tests/api_contracts.rs 
  --app-factory crate::create_app
```

This generates a test file that:

1. Initializes your App factory.
2. Iterates through every route in your OpenAPI spec.
3. Sensibly mocks requests.
4. Validates that your Rust implementation returns the headers and bodies defined in the YAML.

## 4. Schema Generation (Rust ➔ OpenAPI)

Emit a minimal OpenAPI 3.2 document from a Rust struct/enum with optional `info` metadata.

```bash
cargo run -p cdd-cli -- schema-gen 
  --source-path web/src/models/user.rs 
  --name User 
  --openapi 
  --info-title "User API" 
  --info-version "1.0.0" 
  --info-summary "User service schema" 
  --info-terms-of-service "https://example.com/terms" 
  --info-contact-name "API Support" 
  --info-contact-email "support@example.com" 
  --info-license-name "Apache 2.0" 
  --info-license-identifier "Apache-2.0"
```
