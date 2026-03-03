# Publishing `cdd-rust`

## Publishing to crates.io

To publish `cdd-rust` (and its sub-crates like `cdd-core` and `cdd-cli`) to crates.io, follow these steps:

1.  **Login to crates.io:**
    Ensure you have an account on [crates.io](https://crates.io/) and have created an API token.
    Run `cargo login` and paste your token.

2.  **Verify the Build:**
    Ensure everything builds and tests pass.
    ```bash
    cargo check
    cargo test
    ```

3.  **Publish Sub-crates First:**
    Publish the dependencies in the correct order (e.g., `core` before `cli`).
    ```bash
    cd core
    cargo publish
    cd ../cli
    cargo publish
    ```

## Publishing Documentation

### To docs.rs (Automatically)

When you publish a crate to crates.io, its documentation is automatically built and hosted on [docs.rs](https://docs.rs). You do not need to do anything manually for this.

### To Your Own Server

To build the documentation as static HTML files for hosting on your own server (like GitHub Pages, Netlify, or AWS S3):

1.  **Build Docs Locally:**
    ```bash
    cargo doc --no-deps --document-private-items
    ```
2.  **Locate the Output:**
    The static HTML files will be located in the `target/doc` directory.
3.  **Upload:**
    You can copy this folder to any static web host. For example, using `scp`:
    ```bash
    scp -r target/doc/* user@your-server.com:/var/www/html/docs/
    ```
