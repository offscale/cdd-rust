cdd-rust
========

[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![CI/CD](https://github.com/offscale/cdd-rust/workflows/CI/badge.svg)](https://github.com/offscale/cdd-rust/actions)
[![Coverage](https://img.shields.io/badge/Coverage-100%25-brightgreen.svg)](https://github.com/offscale/cdd-rust/actions/workflows/ci-cargo.yml)

OpenAPI ↔ Rust. This is one compiler in a suite, all focussed on the same task: Compiler Driven Development (CDD).

Each compiler is written in its target language, is whitespace and comment sensitive, and has both an SDK and CLI.

The CLI—at a minimum—has:
- `cdd_rust --help`
- `cdd_rust --version`
- `cdd_rust sync`
- `cdd_rust scaffold`
- `cdd_rust to_docs_json --no-imports --no-wrapping -i spec.json`

The goal of this project is to enable rapid application development without tradeoffs. Tradeoffs of Protocol Buffers / Thrift etc. are an untouchable "generated" directory and package, compile-time and/or runtime overhead. Tradeoffs of Java or JavaScript for everything are: overhead in hardware access, offline mode, ML inefficiency, and more. And neither of these alterantive approaches are truly integrated into your target system, test frameworks, and bigger abstractions you build in your app. Tradeoffs in CDD are code duplication (but CDD handles the synchronisation for you).

## 🚀 Capabilities

The `cdd-rust` compiler leverages a unified architecture to support various facets of API and code lifecycle management.

* **Compilation**:
  * **OpenAPI → `Rust`**: Generate idiomatic native models, network routes, test payloads, database schemas, and boilerplate directly from OpenAPI (`.json` / `.yaml`) specifications.
  * **`Rust` → OpenAPI**: Statically parse existing `Rust` source code and emit compliant OpenAPI specifications.
* **AST-Driven & Safe**: Employs static analysis (Abstract Syntax Trees) instead of unsafe dynamic execution or reflection, allowing it to safely parse and emit code even for incomplete or un-compilable project states.
* **Seamless Sync**: Keep your docs, tests, database, clients, and routing in perfect harmony. Update your code, and generate the docs; or update the docs, and generate the code.

## 📦 Installation

Requires a standard Rust toolchain. You can build the CLI and the workspace by running:

```bash
cargo build --release
```

To install the CLI locally:
```bash
cargo install --path cli
```

## 🛠 Usage

### Command Line Interface

Generate scaffolding for your Actix-Web handlers directly from an OpenAPI specification:

```bash
cdd_rust scaffold --input openapi.yaml --output src/handlers
```

Keep your Rust models in sync with your Postgres database and OpenAPI specification:

```bash
cdd_rust sync --db-url postgres://user:pass@localhost/db --output-dir src/models --openapi-path openapi.yaml
```

Generate a docs JSON with snippet examples of the API:

```bash
cdd_rust to_docs_json -i openapi.yaml
```

### Programmatic SDK / Library

You can use the `cdd_core` library programmatically to parse and interact with your OpenAPI contracts and source ASTs:

```rust
use cdd_core::openapi::parse::document::parse_openapi_document;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let yaml = fs::read_to_string("openapi.yaml")?;
    let doc = parse_openapi_document(&yaml)?;
    
    for route in doc.routes {
        println!("Found route: {} {}", route.method, route.path);
    }
    
    Ok(())
}
```

## Design choices

`cdd-rust` uses `ra_ap_syntax` (from rust-analyzer) to read and manipulate the Abstract Syntax Tree (AST) losslessly. This enables non-destructive updates ("surgical merging") that preserves developers' comments, whitespace, and formatting while still generating and updating types and handlers precisely according to the spec. We also integrate closely with `dsync` for Database -> Rust mapping and `actix-web` for server routes, to integrate deeply into the Rust ecosystem's most popular tools.

## 🏗 Supported Conversions for Rust

*(The boxes below reflect the features supported by this specific `cdd-rust` implementation)*

| Concept | Parse (From) | Emit (To) |
|---------|--------------|-----------|
| OpenAPI (JSON/YAML) | ✅ | ✅ |
| `Rust` Models / Structs / Types | ✅ | ✅ |
| `Rust` Server Routes / Endpoints | ✅ | ✅ |
| `Rust` API Clients / SDKs | ❌ | ❌ |
| `Rust` ORM / DB Schemas | ✅ | ✅ |
| `Rust` CLI Argument Parsers | ❌ | ❌ |
| `Rust` Docstrings / Comments | ✅ | ✅ |

---

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
