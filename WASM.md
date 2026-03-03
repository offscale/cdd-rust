# WebAssembly (WASM) Support

Currently, compiling this project to WebAssembly (WASM) is **not possible**.

## Reason

The CLI and core library heavily depend on asynchronous networking and multi-threading runtimes provided by `tokio` and `actix-web`. Specifically:
- `actix-web` requires a full multi-threaded Tokio runtime with networking capabilities to scaffold and generate/test the server.
- The underlying `mio` library, which powers Tokio's I/O event loop, does not support WASM targets (`wasm32-unknown-unknown` or `wasm32-unknown-emscripten`) because WebAssembly lacks standard socket/networking APIs.

To support WASM in the future, the project would need to decouple the code generation and AST parsing logic completely from the Actix and Tokio dependencies, or utilize a WASI runtime with networking extensions (like WasmEdge) once they become fully standardized and supported by the Rust async ecosystem.
