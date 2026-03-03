# Developing `cdd-rust`

To develop `cdd-rust`, you will need to clone the repository and run standard Cargo tooling.

1.  **Clone the Repository:**
    ```bash
    git clone https://github.com/offscale/cdd-rust.git
    cd cdd-rust
    ```

2.  **Use Make Targets:**
    The project includes a `Makefile` / `make.bat`. Use it for common operations:
    ```bash
    make build        # Build debug and release
    make test         # Run unit and integration tests across the workspace
    make build_docs   # Compile docs locally
    ```

3.  **Project Structure:**
    -   `core/`: Contains the parsing, emitting, and intermediate representation logic.
    -   `cli/`: The binary interface.
    -   `web/`: Actix-web dummy server used in testing.

4.  **Running CI Locally:**
    We enforce `clippy` and `rustfmt` cleanly on all modules:
    ```bash
    cargo fmt -- --check
    cargo clippy --workspace -- -D warnings
    ```
