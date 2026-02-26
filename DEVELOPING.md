# Developer Guide

Install the latest version of [Rust](https://www.rust-lang.org). We tend to use nightly
versions (required for `ra_ap_syntax` compatibility feature flags in some contexts). [CLI tool for installing Rust](https://rustup.rs).

We use [rust-clippy](https://github.com/rust-lang-nursery/rust-clippy) linters to improve code quality.

## Prerequisites

* Rust (Nightly toolchain)
* PostgreSQL (if running the reference web implementation)

## Step-by-step guide

```bash
# Install Rust (nightly) 
$ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly

# Install cargo-make (cross-platform feature-rich reimplementation of Make) 
$ cargo install --force cargo-make

# Install rustfmt (Rust formatter) 
$ rustup component add rustfmt

# Clone this repo
$ git clone https://github.com/offscale/cdd-rust && cd cdd-rust

# Build the project
$ cargo build

# Run unit tests
$ cargo test

# Run the generated contract tests (requires web/tests/api_contracts.rs to be generated)
$ cargo test -p cdd-web

# Format, build and test
$ cargo make
```
