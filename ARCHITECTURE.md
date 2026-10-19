# cdd-rust Architecture

<!-- BADGES_START -->
[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![CI/CD](https://github.com/SamuelMarks/cdd-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/SamuelMarks/cdd-rust/actions)
<!-- BADGES_END -->

The **cdd-rust** tool acts as a dedicated lossless compiler and transpiler. Its fundamental architecture follows standard compiler design principles, divided into three distinct phases: **Frontend (Parsing)**, **Intermediate Representation (IR)**, and **Backend (Emitting & Surgical Patching)**.

This decoupled design ensures that any format capable of being parsed into the IR can subsequently be emitted or patched into any supported output format—whether that is an Actix-Web or Axum server route, a Reqwest client SDK, a Clap CLI, a Diesel ORM model, an Arazzo 1.1.0 workflow orchestrator, an MCP adapter, or an OpenAPI 3.2.0 specification.

## 🏗 High-Level Overview

```mermaid
graph TD
    %% Styling Definitions
    classDef frontend fill:#57caff,stroke:#4285f4,stroke-width:2px,color:#20344b,font-family:Roboto Mono
    classDef core fill:#ffd427,stroke:#f9ab00,stroke-width:3px,color:#20344b,font-family:Google Sans,font-weight:bold
    classDef backend fill:#5cdb6d,stroke:#34a853,stroke-width:2px,color:#20344b,font-family:Roboto Mono
    classDef endpoint fill:#ffffff,stroke:#20344b,stroke-width:1px,color:#20344b,font-family:Google Sans

    subgraph Frontend [Parsers]
        A[OpenAPI / Swagger / Arazzo]:::endpoint --> P1(Spec Parser):::frontend
        B[Rust Models / Source]:::endpoint --> P2(ra_ap_syntax CST Parser):::frontend
        C[Actix-Web & Axum Routes]:::endpoint --> P3(Strategy Route Parser):::frontend
        D[Reqwest SDK & Clap CLI]:::endpoint --> P4(Client & Workflow Parser):::frontend
    end

    subgraph Core [Intermediate Representation]
        IR((CDD IR & Semantic Diff)):::core
    end

    subgraph Backend [Emitters & CST Patchers]
        E1(OpenAPI & Arazzo Emitter):::backend --> X[OpenAPI 3.2.0 & Arazzo 1.1.0]:::endpoint
        E2(CST Struct & Enum Patcher):::backend --> Y[Rust Models / Structs]:::endpoint
        E3(Server Strategy Emitter):::backend --> Z[Actix-Web & Axum Controllers]:::endpoint
        E4(Client Strategy Emitter):::backend --> W[Reqwest SDKs & Workflows]:::endpoint
        E5(ORM, CLI & MCP Emitter):::backend --> V[Diesel ORM / Clap CLI / MCP]:::endpoint
    end

    P1 --> IR
    P2 --> IR
    P3 --> IR
    P4 --> IR

    IR --> E1
    IR --> E2
    IR --> E3
    IR --> E4
    IR --> E5
```

## 🧩 Core Components

### 1. The Frontend (Parsers)

The Frontend's responsibility is to read an input source and translate it into the universal CDD Intermediate Representation (IR).

* **Static Analysis (CST-Driven via `ra_ap_syntax`)**: For `Rust` source code, the tool **does not** use dynamic reflection or execute the code. Instead, it parses source files into a lossless Concrete Syntax Tree (CST) using `ra_ap_syntax` (`rust-analyzer`'s syntax tree engine for Rust Edition 2021), preserving whitespace, comments, docstrings, and attributes while extracting structs, enums, functions, type signatures, Reqwest client calls, Actix-Web/Axum server routes, and Arazzo workflow steps.
* **OpenAPI, Swagger & Arazzo Parsing**: For OpenAPI 3.2.0, Swagger 2.0, JSON Schema (2020-12), and Arazzo 1.1.0 inputs, the parser normalizes legacy specifications, resolves internal and external `$ref`s (`DocumentRegistry`), and extracts models, routes, security schemes, webhooks, callbacks, and multi-step workflow graphs into the IR.

### 2. Intermediate Representation (IR) & Semantic Diffing

The Intermediate Representation (`cdd-core`) is the normalized, language-agnostic data structure representing:
* **Models (`ParsedStruct` / `ParsedEnum` / `JsonSchema`)**: Entities containing typed fields, serde/utoipa attributes, required constraints, defaults, and rustdoc descriptions.
* **Endpoints / Operations (`ParsedRoute`)**: HTTP verbs, paths, path/query/header/cookie parameters, request bodies, security requirements, links, callbacks, webhooks, and responses.
* **Workflows (`ArazzoDocument` / `Workflow` / `Step`)**: Multi-step operation DAGs, runtime expressions, criteria, and success/failure actions.
* **Structural Diffs (`StructDiff` / `FieldChange`)**: Computed differences between existing CST nodes and target specifications to drive lossless in-place patching.

### 3. The Backend (Emitters & Surgical Patchers)

The Backend takes the universal IR (and optional CST diffs) and emits or patches target outputs via the `BackendStrategy`, `CliStrategy`, and `WorkflowStrategy` traits:

* **Lossless CST Patcher (`cdd_core::classes::patcher`)**: Modifies existing `.rs` files in-place without disturbing user comments, formatting, or custom handler bodies.
* **Server Emitters (`ActixStrategy`, `AxumStrategy`, `McpServerStrategy`)**: Create and update Actix-Web and Axum route handlers, router configurations, MCP server handlers, and contract tests.
* **Client & CLI Emitters (`ReqwestStrategy`, `ClapCliStrategy`, `McpClientStrategy`)**: Create async Reqwest API clients, Arazzo workflow orchestrators, Clap v4 CLI binaries, and MCP client adapters.
* **Database ORM Sync (`DieselMapper`)**: Bridges Diesel `schema.rs` (`dsync`), Rust structs, and OpenAPI schemas.
* **Specification Emitters**: Serialize the IR into compliant OpenAPI 3.2.0, JSON Schema 2020-12, and Arazzo 1.1.0 JSON or YAML documents.

## 🔄 Extensibility

Because of the strategy and IR design, adding support for a new `Rust` framework requires minimal effort:
1. **To support parsing a new framework**: Implement extraction into `ParsedRoute` / `ParsedStruct` in `cdd-core`.
2. **To support emitting a new framework**: Implement the `BackendStrategy` / `WorkflowStrategy` traits in `cdd_core::strategies`.

## 🛡 Design Principles

1. **A Single Source of Truth**: Maintain definitions in whichever format is most ergonomic (OpenAPI/Arazzo files, Rust structs/handlers, or Diesel `schema.rs`) and synchronize the rest.
2. **Zero-Execution Parsing**: Strictly statically analyze inputs without compiling or executing untrusted target code.
3. **Lossless Conversion**: Retain formatting, comments, docstrings, type annotations, and attributes across `Source ⇄ CST/IR ⇄ Target`.
4. **Symmetric Operations**: Every operation and workflow in the IR holds all metadata required to emit both server handlers and client SDK/CLI orchestrators.
