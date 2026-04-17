# cdd-rust

[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![CI/CD](https://github.com/offscale/cdd-rust/workflows/CI/badge.svg)](https://github.com/offscale/cdd-rust/actions)
[![Docs Coverage](https://img.shields.io/badge/docs-100%25-green.svg)](https://docs.rs/cdd-rust)
[![Tests Coverage](https://img.shields.io/badge/tests-100%25-green.svg)](https://crates.io/crates/cdd-rust)

OpenAPI ↔ Rust. This is one compiler in a suite, all focussed on the same task: Compiler Driven Development (CDD).

Each compiler is written in its target language, is whitespace and comment sensitive, and has both an SDK and CLI.

The CLI—at a minimum—has:

- `cdd-rust --help`
- `cdd-rust --version`
- `cdd-rust from_openapi to_sdk_cli -i spec.json`
- `cdd-rust from_openapi to_sdk -i spec.json`
- `cdd-rust from_openapi to_server -i spec.json`
- `cdd-rust to_openapi -f path/to/code`
- `cdd-rust to_docs_json --no-imports --no-wrapping -i spec.json`
- `cdd-rust serve_json_rpc --port 8080 --listen 0.0.0.0`

The goal of this project is to enable rapid application development without tradeoffs. Tradeoffs of Protocol Buffers / Thrift etc. are an untouchable "generated" directory and package, compile-time and/or runtime overhead. Tradeoffs of Java or JavaScript for everything are: overhead in hardware access, offline mode, ML inefficiency, and more. And neither of these alternative approaches are truly integrated into your target system, test frameworks, and bigger abstractions you build in your app. Tradeoffs in CDD are code duplication (but CDD handles the synchronisation for you).

## 🚀 Capabilities

The `cdd-rust` compiler leverages a unified architecture to support various facets of API and code lifecycle management.

- **Compilation**:
    - **OpenAPI → `Rust`**: Generate idiomatic native models, network routes, client SDKs, and boilerplate directly from OpenAPI (`.json` / `.yaml`) specifications.
    - **`Rust` → OpenAPI**: Statically parse existing `Rust` source code and emit compliant OpenAPI specifications.
- **AST-Driven & Safe**: Employs static analysis instead of unsafe dynamic execution or reflection, allowing it to safely parse and emit code even for incomplete or un-compilable project states.
- **Seamless Sync**: Keep your docs, tests, database, clients, and routing in perfect harmony. Update your code, and generate the docs; or update the docs, and generate the code.

## 📦 Installation & Build

### Native Tooling

```bash
cargo build
cargo test
```

### Makefile / make.bat

You can also use the included cross-platform Makefiles to fetch dependencies, build, and test:

```bash
# Install dependencies
make deps

# Build the project
make build

# Run tests
make test
```

## 🛠 Usage

### Command Line Interface

```bash
# Generate Rust models from an OpenAPI spec
cdd-rust from_openapi to_sdk -i spec.json -o src/models

# Generate an OpenAPI spec from your Rust code
cdd-rust to_openapi -f src/models -o openapi.json
```

### Programmatic SDK / Library

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

## 🏗 Supported Conversions for Rust

*(The boxes below reflect the features supported by this specific `cdd-rust` implementation)*

| Features | Parse (From) | Emit (To) |
| --- | --- | --- |
| OpenAPI 3.2.0 | ✅ | ✅ |
| API Client SDK | ✅ | ✅ |
| API Client CLI | ✅ | ✅ |
| Server Routes / Endpoints | ✅ | ✅ |
| ORM / DB Schema | [ ] | [ ] |
| Mocks + Tests | [ ] | [ ] |
| Model Context Protocol (MCP) | [ ] | [ ] |

### Uncommon Features

`cdd-rust` supports extensive backwards compatibility features:
- **Legacy Swagger 2.0 Support:** Natively parses and processes legacy `swagger: "2.0"` specifications in addition to OpenAPI 3.x, ensuring seamless backwards compatibility and bridging older APIs into the modern Rust ecosystem.

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
