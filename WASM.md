# WebAssembly (WASM) Support

The core tooling of this project can be compiled to WebAssembly (WASM) via the `wasm32-wasip1` target.

## Building for WASM

Because `actix-web`, `tokio`, and `ureq` rely on system-level networking and threading primitives that are not available in standard WASM environments, the WASM build disables those default features (`server`, `client`).

To build the WASM binary:

```bash
# Add the target
rustup target add wasm32-wasip1

# Build without default features
cargo build -p cdd-cli --release --target wasm32-wasip1 --no-default-features
```

## Running the WASM binary

You can run the compiled binary using a WASI-compliant runtime like [Wasmtime](https://wasmtime.dev/):

```bash
wasmtime target/wasm32-wasip1/release/cdd-rust.wasm --help
```
