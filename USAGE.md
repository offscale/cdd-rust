# Using cdd-rust

You can invoke `cdd-rust` via the CLI to parse OpenAPI specifications into Rust code or to parse Rust source code into OpenAPI specifications.

```sh
# Help
cdd-rust --help

# Generate OpenAPI from Rust
cdd-rust to_openapi -f path/to/rust/code -o spec.json

# Generate a Rust CLI client from OpenAPI
cdd-rust from_openapi to_sdk_cli -i spec.json -o ./client

# Generate a JSON representation for docs
cdd-rust to_docs_json --no-imports --no-wrapping -i spec.json
```