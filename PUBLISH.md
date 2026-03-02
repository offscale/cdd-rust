# Publishing cdd-rust

## Crates.io

To publish `cdd-rust` to Crates.io:

1. Update the version in `Cargo.toml`.
2. Ensure you are logged into `cargo` with `cargo login`.
3. Run `cargo publish` in the root workspace, or specifically in `core` then `cli`.

## Documentation

To generate and publish the documentation:

1. Run `make build_docs`.
2. The output will be in `target/doc/`.
3. Serve this locally with a static web server or push it to a hosting provider.