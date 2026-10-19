# Using `cdd-rust`

The primary CLI interaction uses the `cdd-rust` executable, and the programmatic SDK is exposed via the `cdd_cli` and `cdd_core` crates.

## Command Overview

*   `cdd-rust from_openapi <command> -i <input.yaml> -o <out_dir>`
    Generate code starting from an OpenAPI 3.2.0 or Swagger 2.0 specification (or batch directory via `-d/--input-dir`):
    *   `to_sdk`: Generate a Reqwest API client SDK.
    *   `to_sdk_cli`: Generate an offline-first Clap CLI wrapping the Client SDK.
    *   `to_server`: Generate an Actix-Web (`--framework actix-web`) or Axum (`--framework axum`) server scaffold.

*   `cdd-rust to_openapi -i <src_dir> -o <spec.yaml>`
    Statically parse an existing Actix-Web, Axum, or Reqwest Rust workspace/file and emit an OpenAPI 3.2.0 specification.

*   `cdd-rust from_arazzo <command> -i <arazzo.yaml> -o <out_dir>`
    Generate multi-step workflow code starting from an Arazzo 1.1.0 specification:
    *   `to_sdk`: Generate a Reqwest Client SDK workflow orchestrator (`WorkflowsClient`).
    *   `to_sdk_cli`: Generate a Clap CLI workflow runner that directly reuses the Client SDK.
    *   `to_server`: Generate Actix-Web or Axum server workflow handlers and E2E integration tests.

*   `cdd-rust to_arazzo -i <src_dir> -o <arazzo.yaml>`
    Statically extract Arazzo 1.1.0 workflows from Rust source code (SDK, CLI, or Server tests).

*   `cdd-rust sync [--truth <db|api|code>] -i <schema.rs> -o <models_dir>`
    Synchronize Diesel database schemas (`schema.rs`), Rust models, JSON Schemas, and OpenAPI specifications in-place using lossless CST patching.

*   `cdd-rust to_docs_json -i <spec.yaml> -o <docs.json> [--no-imports] [--no-wrapping]`
    Emit structured JSON containing Reqwest (`default`), `actix`, and `axum` code snippets for every operation and webhook in an OpenAPI specification.

*   `cdd-rust serve_json_rpc --port 8080 --listen 127.0.0.1`
    Host the full CLI capability as a JSON-RPC 2.0 HTTP interface.

*   `cdd-rust mcp`
    Run the compiler as a Model Context Protocol (MCP) server over `stdio`.

*   `cdd-rust scaffold -i <openapi.yaml> -o <handlers_dir> [--route-config-path <routes.rs>]`
    Standalone route and handler scaffolder that patches handler modules and injects route registrations in-place.

*   `cdd-rust test-gen -i <openapi.yaml> -o <tests/api_contracts.rs> [--app-factory <path>]`
    Standalone contract test generator for Actix-Web, Axum, Reqwest, or Clap CLI targets.

*   `cdd-rust schema-gen -i <model.rs> --name <StructOrEnumName> [-o <schema.json>] [--openapi]`
    Standalone Rust-to-JSON-Schema (draft 2020-12) or minimal OpenAPI 3.2.0 reflection compiler.

## Example: Code First to API

1. Define handlers with `actix-web` macros (`#[get]`, `#[post]`) or `axum` `Router` routes and `serde` / `utoipa` models in your project.
2. Run `cdd-rust to_openapi -i src -o openapi.yaml --target server-actix`.
3. An `openapi.yaml` file natively reflecting your Rust structs, docstrings, and handlers is created.

## Example: API First to Code

1. Start from a Swagger 2.0 or OpenAPI 3.2.0 YAML/JSON specification.
2. Run `cdd-rust from_openapi to_server -i openapi.yaml -o my_app --framework actix-web --tests --mcp`.
3. The tool generates non-destructive handler scaffolding, models, MCP server adapters, and contract tests in `my_app/`.
