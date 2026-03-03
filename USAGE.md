# Using `cdd-rust`

The primary CLI interaction uses the `cdd-rust` executable.

## Command Overview

*   `cdd-rust from_openapi <command> -i <input.yaml> -o <out_dir>`
    Generate code starting from an OpenAPI specification.
    Commands include:
    *   `to_sdk_cli`: Generate an offline-first Clap CLI SDK.
    *   `to_sdk`: Generate a Reqwest API client.
    *   `to_server`: Generate an Actix-web server scaffold.

*   `cdd-rust to_openapi -f <src_dir> -o <spec.yaml>`
    Parse an existing Actix-web or generic Rust workspace and build an OpenAPI spec.

*   `cdd-rust to_docs_json -i <spec.yaml> -o <docs.json>`
    Format the spec into a JSON format used directly by the central CDD documentation site.

*   `cdd-rust serve_json_rpc --port 8082`
    Host the full CLI capability as a JSON-RPC 2.0 interface.

## Example: Code First to API

1. Define a basic handler with `actix-web` macros in your project.
2. Run `cdd-rust to_openapi -f src/api -o output.yaml`.
3. An `output.yaml` file natively reflecting your Rust structs and handlers is created.

## Example: API First to Code

1. Use an OpenAPI 3.2.0 yaml or json spec.
2. Run `cdd-rust from_openapi to_server -i openapi.yaml -o my_actix_app`.
3. The tool generates Actix scaffolding in `my_actix_app/handlers/`, Diesel models in `my_actix_app/models/`, and tests in `my_actix_app/tests/`.
