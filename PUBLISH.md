# Publishing `cdd-rust`

This guide explains how to publish the `cdd-rust` packages to the public Rust registry (crates.io) and how to manage and publish its documentation.

## Publishing to crates.io

Rust's official package registry is [crates.io](https://crates.io/).

### Prerequisites
1. Create an account on crates.io using your GitHub account.
2. Go to your Account Settings and create an API token.
3. Log in locally using Cargo:
   ```bash
   cargo login <your-api-token>
   ```

### Publishing Steps

Since this project is a workspace (containing `core`, `cli`, and `web`), you must publish the crates in dependency order. Typically, `core` first, followed by `cli`. The `web` application usually isn't published to crates.io unless it's intended to be a reusable library.

1. **Verify the build and tests:**
   ```bash
   cargo test --all
   cargo check --all
   ```

2. **Publish the `core` crate:**
   ```bash
   cd core
   cargo publish
   cd ..
   ```

3. **Publish the `cli` crate:**
   Wait a few moments for the `core` crate to propagate on crates.io, then publish the CLI:
   ```bash
   cd cli
   cargo publish
   cd ..
   ```

*Note: You can use the `--dry-run` flag with `cargo publish` to verify everything is correct before actually publishing.*

## Publishing Documentation

### 1. Most Popular Location: docs.rs (Automatic)

When you publish a crate to crates.io, its documentation is **automatically built and hosted** on [docs.rs](https://docs.rs/). There is no manual upload step required.

If you have specific features that need to be enabled for the docs to build correctly, you can add this metadata to your root or package-level `Cargo.toml`:

```toml
[package.metadata.docs.rs]
all-features = true
rustdoc-args = ["--cfg", "docsrs"]
```

### 2. Hosting Docs on Your Own Server (Static Serving)

You can easily generate a static HTML version of your crate's documentation to host on your own infrastructure.

1. **Generate the documentation:**
   The `--no-deps` flag ensures you are only generating docs for your crates, not all third-party dependencies.
   ```bash
   cargo doc --no-deps --workspace
   ```

2. **Locate the static files:**
   The generated HTML, CSS, and JS files will be located in the `target/doc/` directory.

3. **Test the static docs locally:**
   You can serve this folder using any static web server. For example, using Python:
   ```bash
   python3 -m http.server 8000 -d target/doc
   ```
   Then navigate to `http://localhost:8000/<crate_name>/` in your browser.

4. **Upload to your server:**
   You can simply copy the contents of `target/doc/` to your web server (e.g., NGINX, Apache, AWS S3, GitHub Pages) and serve them statically.
