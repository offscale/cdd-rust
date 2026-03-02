cdd-rust
============

[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![CI/CD](https://github.com/offscale/cdd-rust/workflows/CI/badge.svg)](https://github.com/offscale/cdd-rust/actions)
[![Test Coverage](https://img.shields.io/badge/test%20coverage-100%25-brightgreen.svg)](https://github.com/offscale/cdd-rust/actions)
[![Doc Coverage](https://img.shields.io/badge/doc%20coverage-100%25-brightgreen.svg)](https://github.com/offscale/cdd-rust/actions)

OpenAPI ↔ Rust. This is one compiler in a suite, all focussed on the same task: Compiler Driven Development (CDD).

Each compiler is written in its target language, is whitespace and comment sensitive, and has both an SDK and CLI.

The CLI—at a minimum—has:
- `cdd-rust --help`
- `cdd-rust --version`
- `cdd-rust from_openapi -i spec.json`
- `cdd-rust to_openapi -f path/to/code`
- `cdd-rust to_docs_json --no-imports --no-wrapping -i spec.json`

The goal of this project is to enable rapid application development without tradeoffs. Tradeoffs of Protocol Buffers / Thrift etc. are an untouchable "generated" directory and package, compile-time and/or runtime overhead. Tradeoffs of Java or JavaScript for everything are: overhead in hardware access, offline mode, ML inefficiency, and more. And neither of these alterantive approaches are truly integrated into your target system, test frameworks, and bigger abstractions you build in your app. Tradeoffs in CDD are code duplication (but CDD handles the synchronisation for you).

## 🚀 Capabilities

The `cdd-rust` compiler leverages a unified architecture to support various facets of API and code lifecycle management.

* **Compilation**:
  * **OpenAPI → `Rust`**: Generate idiomatic native models, network routes, client SDKs, database schemas, and boilerplate directly from OpenAPI (`.json` / `.yaml`) specifications.
  * **`Rust` → OpenAPI**: Statically parse existing `Rust` source code and emit compliant OpenAPI specifications.
* **AST-Driven & Safe**: Employs static analysis (Abstract Syntax Trees) instead of unsafe dynamic execution or reflection, allowing it to safely parse and emit code even for incomplete or un-compilable project states.
* **Seamless Sync**: Keep your docs, tests, database, clients, and routing in perfect harmony. Update your code, and generate the docs; or update the docs, and generate the code.

## 📦 Installation

Requires Rust toolchain (1.75+).

```sh
# Clone the repository
git clone https://github.com/offscale/cdd-rust.git
cd cdd-rust

# Install the CLI tool
cargo install --path cli

# Run to verify
cdd-rust --version
```

## 🛠 Usage

### Command Line Interface

```sh
# Generate OpenAPI spec from existing Rust code
cdd-rust to_openapi -f ./my_project/src -o spec.json

# Generate a Rust client SDK from an OpenAPI spec
cdd-rust from_openapi to_sdk -i spec.json -o ./my_sdk_client

# Serve a JSON-RPC compiler interface
cdd-rust serve_json_rpc --port 8082 --listen 0.0.0.0
```

### Programmatic SDK / Library

```rust
use cdd_core::openapi::parse::parse_openapi;
use cdd_core::classes::emit::emit_rust_structs;

fn main() {
    let spec = parse_openapi("spec.json").unwrap();
    let rust_code = emit_rust_structs(&spec);
    println!("{}", rust_code);
}
```

## Design choices

The tool utilizes `ra_ap_syntax` (rust-analyzer's parsing logic) to create loss-less syntax trees of Rust code. This is essential for CDD, where modifications to source files must preserve existing code styles, whitespace, and non-target modifications. By sidestepping `syn` which drops non-semantic tokens, `cdd-rust` provides powerful code editing without breaking user styling.
Furthermore, we adhere to a strictly layered architecture parsing source into a universal intermediate representation before emission, allowing N×M compatibility.

## 🏗 Supported Conversions for Rust

*(The boxes below reflect the features supported by this specific `cdd-rust` implementation)*

| Concept | Parse (From) | Emit (To) |
|---------|--------------|-----------|
| OpenAPI (JSON/YAML) | [✅] | [✅] |
| `Rust` Models / Structs / Types | [✅] | [✅] |
| `Rust` Server Routes / Endpoints | [✅] | [✅] |
| `Rust` API Clients / SDKs | [✅] | [✅] |
| `Rust` ORM / DB Schemas | [✅] | [✅] |
| `Rust` CLI Argument Parsers | [✅] | [✅] |
| `Rust` Docstrings / Comments | [✅] | [✅] |

---

## License

Licensed under either of

- Apache License, Version 2.0 (LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0)
- MIT license (LICENSE-MIT or https://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
