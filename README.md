cdd-rust
========
[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![interactive WASM web demo](https://img.shields.io/badge/interactive-WASM_web_demo-blue.svg)](https://offscale.io/wasm_web_demo)
[![CI](https://github.com/offscale/cdd-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/offscale/cdd-rust/actions)
[![Test Coverage](https://img.shields.io/badge/test_coverage-77.24%25-yellow.svg)](#)
[![Doc Coverage](https://img.shields.io/badge/doc_coverage-100.00%25-brightgreen.svg)](#)

----

OpenAPI ↔ Rust. This is one compiler in a suite, all focussed on the same task: Compiler Driven Development (CDD).

Each compiler is written in its target language, is whitespace and comment sensitive, and has both an SDK and CLI.

The core philosophy of Compiler Driven Development (CDD) is synchronization without compromise. Where traditional generators silo your API boundaries into read-only files, this compiler natively merges changes into your codebase via a robust, [whitespace and comment aware] Abstract Syntax Tree (AST) driven parser & emitter. It bridges the gap between design and implementation, allowing you to seamlessly generate SDKs from a spec or extract a spec from existing code. By keeping your APIs, SDKs, and tests in continuous, automated alignment, it drastically improves both delivery speed and software reliability.

The CLI—at a minimum—has:

- `cdd-rust --help`
- `cdd-rust --version`
- `cdd-rust from_openapi to_sdk_cli -i spec.json`
- `cdd-rust from_openapi to_sdk -i spec.json`
- `cdd-rust from_openapi to_server -i spec.json`
- `cdd-rust to_openapi -i path/to/code`
- `cdd-rust to_docs_json --no-imports --no-wrapping -i spec.json`
- `cdd-rust serve_json_rpc -p 8080 -l 0.0.0.0`

## SDK Example

```rs
use cdd_core::openapi::parse::parse_openapi_spec;
use cdd_core::classes::emit::generate_dtos;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec = fs::read_to_string("spec.yaml")?;
    let models = parse_openapi_spec(&spec)?;
    
    // Generate idiomatic Rust structs from the OpenAPI Components/Schemas
    let rust_code = generate_dtos(&models);
    
    fs::write("models.rs", rust_code)?;
    Ok(())
}
```

## Installation

```bash
cargo build
```

## Development

You can use standard Cargo commands or the included cross-platform Makefiles to fetch dependencies, build, and test:

```bash
cargo clippy
cargo test
# or
make deps
make build
make test
# or on Windows
.\make.bat deps
.\make.bat build
.\make.bat test
```

See [PUBLISH.md](PUBLISH.md) for packaging and releasing.

## Features

The `cdd-rust` compiler leverages a unified architecture to support various facets of API and code lifecycle management. For a deep dive into the compiler's design, see [ARCHITECTURE.md](ARCHITECTURE.md).

- **Compilation**:
    - **OpenAPI → `Rust`**: Generate idiomatic native models, network routes, client SDKs, and boilerplate directly from OpenAPI (`.json` / `.yaml`) specifications.
    - **`Rust` → OpenAPI**: Statically parse existing `Rust` source code and emit compliant OpenAPI specifications.
- **AST-Driven & Safe**: Employs static analysis instead of unsafe dynamic execution or reflection, allowing it to safely parse and emit code even for incomplete or un-compilable project states.
- **Seamless Sync**: Keep your docs, tests, database, clients, and routing in perfect harmony. Update your code, and generate the docs; or update the docs, and generate the code.

**Uncommon Features:**

`cdd-rust` supports extensive backwards compatibility features:
- **Legacy Swagger 2.0 Support:** Natively parses and processes legacy `swagger: "2.0"` specifications in addition to OpenAPI 3.x, ensuring seamless backwards compatibility and bridging older APIs into the modern Rust ecosystem.

## CLI Options

```text
CDD Toolchain CLI

Usage: cdd-rust [OPTIONS] <COMMAND>

Commands:
  sync            Synchronize DB schema to Rust models and OpenAPI-ready structs
  test-gen        Generates integration tests based on OpenAPI contracts
  scaffold        Scaffolds handler functions from OpenAPI Routes
  schema-gen      Generates a JSON Schema from a Rust struct or enum
  to_docs_json    Generate JSON documentation with code snippets for an OpenAPI specification.
  from_openapi    Generate code from an OpenAPI specification.
  to_openapi      Generate an OpenAPI specification from source code.
  serve_json_rpc  Expose CLI interface as a JSON-RPC server.
  help            Print this message or the help of the given subcommand(s)

Options:
  -t, --target <TARGET>
          Target mode (server or client)

          Possible values:
          - server-actix: Generate Actix Web server scaffolding
          - server-axum:  Generate Axum server scaffolding
          - client:       Generate Reqwest client scaffolding
          - cli:          Generate Clap CLI scaffolding
          
          [env: CDD_TARGET=]
          [default: server-actix]

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

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
