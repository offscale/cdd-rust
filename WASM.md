# WASM Support

`cdd-rust` supports WebAssembly (WASM) as a primary build target to enable:
1. Unified CLI runnable on systems without language runtimes
2. Unified Web Interface for `cdd-*` components.

## Status

| WASM Target | Supported | Notes |
|-------------|-----------|-------|
| `wasm32-unknown-unknown` | ✅ | Core parsing and generation logic compiles to WASM out of the box. |

## Building

To build the project for WASM:

```sh
make build_wasm
# or manually:
cargo build --target wasm32-unknown-unknown --release
```

The output will be placed in `target/wasm32-unknown-unknown/release/`.