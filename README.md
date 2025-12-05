cdd-rust: OpenAPI ↔ Rust
========================
[![Rust: nightly](https://img.shields.io/badge/Rust-nightly-blue.svg)](https://www.rust-lang.org) 
[![License: (Apache-2.0 OR MIT)](https://img.shields.io/badge/LICENSE-Apache--2.0%20OR%20MIT-orange)](LICENSE-APACHE) 
[![CI](https://github.com/offscale/cdd-rust/actions/workflows/ci-cargo.yml/badge.svg)](https://github.com/offscale/cdd-rust/actions/workflows/ci-cargo.yml) 

**Compiler Driven Development (CDD)** for Rust. 

**cdd-rust** creates a symbiotic link between your **Database**, **Rust Code**, and **OpenAPI Specifications**. Unlike traditional generators that overwrite files or hide code in "generated" directories, `cdd-rust` uses advanced AST parsing (`ra_ap_syntax`) to surgically patch your *existing* source files, strictly typed handlers, and integration tests. 

Uniquely, `cdd-rust` is built on a **strategy-based architecture**. While the default implementation provides a robust **Actix Web + Diesel** workflow, the core logic is decoupled into strategies and mappers, making it extensible to other ecosystems (e.g., Axum, SQLx) in the future.

It supports two distinct workflows: 
1.  **Scaffold | Patch (OpenAPI ➔ Rust):** Generate/update handlers, routes, and models via `BackendStrategy`.
2.  **Reflect & Sync (Rust ➔ OpenAPI):** Generate OpenAPI specifications from your source code and DB via `ModelMapper`.

## ⚡️ The CDD Loop

```mermaid
%%{init: { 
  'theme': 'base', 
  'themeVariables': { 
    'primaryColor': '#ffffff', 
    'primaryTextColor': '#20344b', 
    'primaryBorderColor': '#20344b', 
    'lineColor': '#20344b', 
    'fontFamily': 'Google Sans, sans-serif' 
  } 
}}%% 

graph TD
%% --- Section 1: Route Logic --- 

%% Node: OpenAPI Path
    OasPath("<strong>OpenAPI Path (YAML)</strong><br/>/users/{id}:<br/>&nbsp;&nbsp;get:<br/>&nbsp;&nbsp;&nbsp;&nbsp;parameters:<br/>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;- name: id<br/>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;in: path<br/>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;schema: {format: uuid}"):::yellow

%% Node: Actix Handler
    Actix("<strong>Handler (Rust)</strong><br/>async fn get_user(<br/>&nbsp;&nbsp;id: web::Path&lt;Uuid&gt;<br/>) -> impl Responder {<br/>&nbsp;&nbsp;/* Logic */<br/>}"):::blue

%% Flow: Down (Scaffold) & Up (Reflect) 
    OasPath ==>|"1. SCAFFOLD / PATCH<br/>(Injects Handler & Route Signature)"| Actix
    Actix -.->|"2. REFLECT / GENERATE<br/>(Extracts Paths via AST)"| OasPath

%% --- Spacer to force vertical layout --- 
    Actix ~~~ OasSchema

%% --- Section 2: Data Models --- 

%% Node: OpenAPI Schema
    OasSchema("<strong>OpenAPI Schema (YAML)</strong><br/>components:<br/>&nbsp;&nbsp;schemas:<br/>&nbsp;&nbsp;&nbsp;&nbsp;User:<br/>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;type: object<br/>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;properties:<br/>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;email: {type: string}"):::yellow

%% Node: Diesel Model
    Diesel("<strong>Data Model (Rust)</strong><br/>#[derive(ToSchema)]<br/>struct User {<br/>&nbsp;&nbsp;id: Uuid,<br/>&nbsp;&nbsp;email: String<br/>}"):::green

%% Flow: Down (Scaffold) & Up (Reflect) 
    OasSchema ==>|"1. SCAFFOLD / PATCH<br/>(Injects Fields & Types)"| Diesel
    Diesel -.->|"2. REFLECT / GENERATE<br/>(Derives Attributes)"| OasSchema

%% --- Styles --- 
    classDef yellow fill:#f9ab00,stroke:#20344b,stroke-width:2px,color:#ffffff,font-family:'Consolas',font-size:14px,text-align:left; 
    classDef blue fill:#4285f4,stroke:#20344b,stroke-width:2px,color:#ffffff,font-family:'Consolas',font-size:14px,text-align:left; 
    classDef green fill:#34a853,stroke:#20344b,stroke-width:2px,color:#ffffff,font-family:'Consolas',font-size:14px,text-align:left; 

    linkStyle default stroke:#20344b,stroke-width:2px; 
```

## 🏗 Architecture

The internal architecture separates the core AST/OpenAPI parsing logic from the target code generation. This allows the tool to support multiple web frameworks and ORMs through the `BackendStrategy` and `ModelMapper` traits.

```mermaid
graph LR
%% --- NODES --- 
    InputOAS(<strong>OpenAPI Spec</strong><br/><em>YAML</em>)
    InputSrc(<strong>Rust Source</strong><br/><em>Files / Schema</em>)

    subgraph Core [Layer 1: Core]
        direction TB
        P_OAS(<strong>OAS Parser</strong><br/><em>serde_yaml</em>)
        P_AST(<strong>AST Parser</strong><br/><em>ra_ap_syntax</em>)
    end

    subgraph Analysis [Layer 2: Analysis]
        IR(<strong>Intermediate Representation</strong><br/><em>ParsedRoute / ParsedStruct</em>)
    end

    subgraph Gen [Layer 3: Generation]
        Base(<strong>Generator Engine</strong><br/><em>Traits: BackendStrategy & ModelMapper</em>)

    %% The Fork
        subgraph Targets [Targets]
            direction TB
            T_Actix(<strong>Actix</strong><br/><em>ActixStrategy</em>)
            T_Diesel(<strong>Diesel</strong><br/><em>DieselMapper</em>)
            T_OutputOAS(<strong>OpenAPI</strong><br/><em>Spec Generation</em>)
            T_Future(<strong>Axum / SQLx</strong><br/><em>Future Strategies</em>)
        end
    end

%% --- EDGES --- 
    InputOAS --> P_OAS
    InputSrc --> P_AST

    P_OAS --> IR
    P_AST --> IR

    IR --> Base

    Base -- "Scaffold / Test" --> T_Actix
    Base -- "Sync Models" --> T_Diesel
    Base -- "Reflect" --> T_OutputOAS
    Base -. "Extension" .-> T_Future

%% --- STYLING --- 
    classDef blue fill:#4285f4,stroke:#ffffff,color:#ffffff,stroke-width:0px
    classDef yellow fill:#f9ab00,stroke:#ffffff,color:#20344b,stroke-width:0px
    classDef green fill:#34a853,stroke:#ffffff,color:#ffffff,stroke-width:0px
    classDef white fill:#ffffff,stroke:#20344b,color:#20344b,stroke-width:2px
    classDef future fill:#f1f3f4,stroke:#20344b,color:#20344b,stroke-width:2px,stroke-dasharray: 5 5

    class InputOAS,InputSrc white
    class P_OAS,P_AST blue
    class IR yellow
    class Base green
    class T_Actix,T_Diesel,T_OutputOAS white
    class T_Future future
```

--- 

## 🚀 Features

### 1. Framework-Agnostic Scaffolding (OpenAPI ➔ Rust)
Stop manually writing repetitive handler signatures. `cdd-rust` reads your spec and generates strictly typed code based on the active `BackendStrategy`.
*   **Handler Scaffolding:** Transforms OpenAPI paths into `async fn` signatures with correct extractors (currently `ActixStrategy` defaults):
    *   Path variables ➔ `web::Path<Uuid>`
    *   Query strings ➔ `web::Query<Value>`
    *   Request bodies ➔ `web::Json<T>`
*   **Route Registration:** Surgically injects route configuration strings (e.g., `cfg.service(...)`) using AST analysis, preserving existing logic.
*   **Non-Destructive Patching:** Uses [`ra_ap_syntax`](https://docs.rs/ra_ap_syntax/) (official [rust-analyzer](https://github.com/rust-lang/rust-analyzer) parser) to edit files safely.

### 2. Source-of-Truth Reflection (Rust ➔ OpenAPI)
Keep your documentation alive. Your Rust code *is* the spec.
*   **Model Mapper Trait:** Extracts DB Schemas via `ModelMapper` (currently `DieselMapper` wrapping `dsync`) to generate/update Rust structs.
*   **Attribute Injection:** Automatically parses structs and injects `#[derive(ToSchema)]` and `#[serde(...)]` attributes to make models OpenAPI-compatible.
*   **Type Mapping:** Maps Rust types (`Uuid`, `chrono::NaiveDateTime`, `Decimal`) back to OpenAPI formats automatically using the `TypeMapper` module.

### 3. Contract Safety (`test-gen`)
Ensure your implementation actually matches the spec using the same strategies used for code generation.
*   **Test Generation:** Generates `tests/api_contracts.rs` based on your `openapi.yaml`.
*   **Smart Mocking:** Automatically fills request parameters with valid dummy data based on type signatures.
*   **Validation:** Verifies that API responses align with the JSON Schema defined in your spec.

--- 

## 📦 Command Usage

### 1. The Sync Pipeline
**DB ➔ Rust Models ➔ OpenAPI Attributes**
Synchronizes your database schema to your Rust structs using the configured `ModelMapper`.

```bash
cargo run -p cdd-cli -- sync \ 
  --schema-path web/src/schema.rs \ 
  --model-dir web/src/models
```

### 2. The Test Pipeline
**OpenAPI ➔ Integration Tests**
Generates a test suite via `BackendStrategy` (default: Actix) that treats your app as a black box.

```bash
cargo run -p cdd-cli -- test-gen \ 
  --openapi-path docs/openapi.yaml \ 
  --output-path web/tests/api_contracts.rs \ 
  --app-factory crate::create_app
```

--- 

## 🛠 Project Structure

*   **`core/`**: The engine. Contains AST parsers, strategies, and the diff/patch logic.
    *   `strategies.rs`: Defines `BackendStrategy` trait and `ActixStrategy`.
    *   `patcher.rs`: Surgical code editing.
    *   `oas.rs`: Parses OpenAPI YAML into Intermediate Representations.
    *   `handlers/routes/contract_test`: Functional modules utilizing strategies to generate code.
*   **`cli/`**: The workflow runner. Wires up specific strategies to commands.
    *   `generator.rs`: Defines `ModelMapper` and `DieselMapper`.
    *   `main.rs`: Dependency injection root.
*   **`web/`**: Reference implementation. An Actix Web + Diesel project demonstrating the generated code in action.

## 🎨 Design Principles

*   **No Magic Folders:** We generate code you can read, debug, and commit.
*   **Lossless Patching:** We edit your source files without breaking your style.
*   **Pluggable Backend:** Core logic is decoupled from specific frameworks (Actix, Axum, etc.).
*   **Type Safety:** `Uuid`, `chrono`, and `rust_decimal` are first-class citizens.

--- 

## Developer guide

Install the latest version of [Rust](https://www.rust-lang.org). We tend to use nightly versions. [CLI tool for installing Rust](https://rustup.rs).

We use [rust-clippy](https://github.com/rust-lang-nursery/rust-clippy) linters to improve code quality.

### Step-by-step guide

```bash
# Install Rust (nightly) 
$ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly
# Install cargo-make (cross-platform feature-rich reimplementation of Make) 
$ cargo install --force cargo-make
# Install rustfmt (Rust formatter) 
$ rustup component add rustfmt
# Clone this repo
$ git clone https://github.com/offscale/cdd-rust && cd cdd-rust
# Run tests
$ cargo test
# Format, build and test
$ cargo make
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.
