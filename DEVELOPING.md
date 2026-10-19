# Developing `cdd-rust`

To develop `cdd-rust`, clone the repository and use either the cross-platform `Makefile` / `make.bat` targets or standard Cargo tooling.

1.  **Clone the Repository:**
    ```bash
    git clone https://github.com/SamuelMarks/cdd-rust.git
    cd cdd-rust
    ```

2.  **Use Make Targets:**
    The project includes a `Makefile` and `make.bat` for common operations:
    ```bash
    make deps         # Fetch Cargo dependencies
    make build        # Build the release cdd-rust binary
    make test         # Run unit and integration tests across the workspace
    make build_docs   # Compile rustdoc documentation locally
    make build_wasm   # Build the wasm32-wasip1 target
    ```

3.  **Project Structure:**
    -   `core/` (`cdd-core`): Contains the `ra_ap_syntax` CST parser, semantic differ, patcher, OpenAPI 3.2.0 / Swagger 2.0 / Arazzo 1.1.0 / MCP models, and framework strategies (`actix-web`, `axum`, `reqwest`, `clap`).
    -   `cli/` (`cdd-cli` / binary `cdd-rust`): Command-line interface and high-level programmatic SDK.
    -   `web/` (`cdd-web`): Reference multi-tiered Actix-Web + Diesel server supporting stub, production, ephemeral, and fake-seeded execution modes.

4.  **Running CI & Coverage Locally:**
    We enforce `rustfmt`, `clippy`, and 100% test and documentation coverage across all workspace members:
    ```bash
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    python3 scripts/update_coverage.py
    ```
