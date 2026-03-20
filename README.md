cdd-rust
============

[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![CI/CD](https://github.com/offscale/cdd-rust/workflows/CI/badge.svg)](https://github.com/offscale/cdd-rust/actions)
[![Docs Coverage](https://img.shields.io/badge/docs-100%25-green.svg)](https://docs.rs/cdd-rust)
[![Tests Coverage](https://img.shields.io/badge/tests-100%25-green.svg)](https://crates.io/crates/cdd-rust)

OpenAPI ↔ Rust. This is one compiler in a suite, all focussed on the same task: Compiler Driven Development (CDD).

Each compiler is written in its target language, is whitespace and comment sensitive, and has both an SDK and CLI.

The CLI—at a minimum—has:
- `cdd-rust --help`
- `cdd-rust --version`
- `cdd-rust from_openapi -i spec.json`
- `cdd-rust to_openapi -f path/to/code`
- `cdd-rust to_docs_json --no-imports --no-wrapping -i spec.json`

The goal of this project is to enable rapid application development without tradeoffs. Tradeoffs of Protocol Buffers / Thrift etc. are an untouchable "generated" directory and package, compile-time and/or runtime overhead. Tradeoffs of Java or JavaScript for everything are: overhead in hardware access, offline mode, ML inefficiency, and more. And neither of these alternative approaches are truly integrated into your target system, test frameworks, and bigger abstractions you build in your app. Tradeoffs in CDD are code duplication (but CDD handles the synchronisation for you).

## 🚀 Capabilities

The `cdd-rust` compiler leverages a unified architecture to support various facets of API and code lifecycle management.

* **Compilation**:
  * **OpenAPI → `Rust`**: Generate idiomatic native models, network routes, client SDKs, database schemas, and boilerplate directly from OpenAPI (`.json` / `.yaml`) specifications.
  * **`Rust` → OpenAPI**: Statically parse existing `Rust` source code and emit compliant OpenAPI specifications.
* **AST-Driven & Safe**: Employs static analysis (Abstract Syntax Trees via `syn`) instead of unsafe dynamic execution or reflection, allowing it to safely parse and emit code even for incomplete or un-compilable project states.
* **Seamless Sync**: Keep your docs, tests, database, clients, and routing in perfect harmony. Update your code, and generate the docs; or update the docs, and generate the code.

## 📦 Installation

**Prerequisites:** Requires the Rust toolchain (Rust 1.70+ recommended).

You can install `cdd-rust` CLI globally using Cargo:

```bash
cargo install cdd-cli
```

Or build from source:

```bash
git clone https://github.com/offscale/cdd-rust.git
cd cdd-rust
cargo build --release
```

## 🛠 Usage

### Command Line Interface

Generate an Actix-Web server scaffolding from an OpenAPI specification:
```bash
cdd-rust from_openapi to_server -i spec.yaml -o src/api
```

Generate an OpenAPI specification from existing Actix-Web routing code:
```bash
cdd-rust to_openapi -f src/api -o new_spec.yaml
```

Generate a fully-typed offline CLI tool based on your OpenAPI specification:
```bash
cdd-rust from_openapi to_sdk_cli -i spec.yaml -o src/cli
```

### Programmatic SDK / Library

```rust
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

## Design choices

`cdd-rust` leverages `syn` and `quote` for robust, declarative parsing and emitting of Rust Abstract Syntax Trees. This provides extreme resilience against syntax errors in incomplete files. The intermediate representation tightly mirrors the OpenAPI 3.2.0 spec, permitting a lossless translation layer between code and definitions. We've selected Actix-Web as the server target, Diesel for the ORM layer, and Reqwest for the client implementation, as they represent the dominant paradigms in the Rust ecosystem. Unlike other cdd-* tools, the Rust tool relies on macros and attributes to map tightly between OpenAPI parameters and Actix extractors natively.

### Note on WASM

Compiling this project to WebAssembly (WASM) is currently **not possible** natively due to deep asynchronous networking requirements (Tokio/mio) needed for server generation and testing workflows. See [WASM.md](WASM.md) for full context.

## 🏗 Supported Conversions for Rust

*(The boxes below reflect the features supported by this specific `cdd-rust` implementation)*

| Concept | Parse (From) | Emit (To) |
|---------|--------------|-----------|
| OpenAPI (JSON/YAML) | ✅ | ✅ |
| `Rust` Models / Structs / Types | ✅ | ✅ |
| `Rust` Server Routes / Endpoints | ✅ | ✅ |
| `Rust` API Clients / SDKs | ✅ | ✅ |
| `Rust` ORM / DB Schemas | ✅ | ✅ |
| `Rust` CLI Argument Parsers | ✅ | ✅ |
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

## CLI Help

```
$ ./target/release/cdd-rust --help
CDD Toolchain CLI

Usage: cdd-rust [OPTIONS] <COMMAND>

Commands:
  sync            Synchronize DB schema to Rust models and OpenAPI-ready structs
  test-gen        Generates integration tests based on OpenAPI contracts
  scaffold        Scaffolds handler functions from OpenAPI Routes
  schema-gen      Generates a JSON Schema from a Rust struct or enum
  to_docs_json    Generates a JSON output with documentation code snippets for an OpenAPI spec
  from_openapi    Generates code from an OpenAPI specification
  to_openapi      Generates an OpenAPI specification from source code
  serve_json_rpc  Expose CLI interface as JSON-RPC server over HTTP
  help            Print this message or the help of the given subcommand(s)

Options:
  -t, --target <TARGET>
          Target mode (server or client)

          Possible values:
          - server: Generate Actix Web server scaffolding
          - client: Generate Reqwest client scaffolding
          - cli:    Generate Clap CLI scaffolding
          
          [env: CDD_TARGET=]
          [default: server]

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

### `from_openapi`

```
$ ./target/release/cdd-rust from_openapi --help
Generates code from an OpenAPI specification

Usage: cdd-rust from_openapi [OPTIONS] <COMMAND>

Commands:
  to_sdk_cli  Generate a CLI SDK
  to_sdk      Generate a Client SDK
  to_server   Generate Server scaffolding
  help        Print this message or the help of the given subcommand(s)

Options:
  -t, --target <TARGET>
          Target mode (server or client)

          Possible values:
          - server: Generate Actix Web server scaffolding
          - client: Generate Reqwest client scaffolding
          - cli:    Generate Clap CLI scaffolding
          
          [env: CDD_TARGET=]
          [default: server]

  -h, --help
          Print help (see a summary with '-h')
```

### `to_openapi`

```
$ ./target/release/cdd-rust to_openapi --help
Generates an OpenAPI specification from source code

Usage: cdd-rust to_openapi [OPTIONS] --input <INPUT>

Options:
  -i, --input <INPUT>
          Path to the code directory or file to parse
          
          [env: CDD_INPUT=]

  -o, --output <OUTPUT>
          Output file for the generated OpenAPI spec
          
          [env: CDD_OUTPUT=]
          [default: spec.json]

  -t, --target <TARGET>
          Target mode (server or client)

          Possible values:
          - server: Generate Actix Web server scaffolding
          - client: Generate Reqwest client scaffolding
          - cli:    Generate Clap CLI scaffolding
          
          [env: CDD_TARGET=]
          [default: server]

  -h, --help
          Print help (see a summary with '-h')
```

### `to_docs_json`

```
$ ./target/release/cdd-rust to_docs_json --help
Generates a JSON output with documentation code snippets for an OpenAPI spec

Usage: cdd-rust to_docs_json [OPTIONS] --input <INPUT>

Options:
  -i, --input <INPUT>
          Path or URL to the OpenAPI specification
          
          [env: CDD_INPUT=]

      --no-imports
          If provided, omit the imports field in the code object
          
          [env: CDD_NO_IMPORTS=]

      --no-wrapping
          If provided, omit the wrapper_start and wrapper_end fields in the code object
          
          [env: CDD_NO_WRAPPING=]

  -o, --output <OUTPUT>
          Output file for the generated JSON
          
          [env: CDD_OUTPUT=]

  -t, --target <TARGET>
          Target mode (server or client)

          Possible values:
          - server: Generate Actix Web server scaffolding
          - client: Generate Reqwest client scaffolding
          - cli:    Generate Clap CLI scaffolding
          
          [env: CDD_TARGET=]
          [default: server]

  -h, --help
          Print help (see a summary with '-h')
```
