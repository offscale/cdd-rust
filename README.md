cdd-rust
--------
[![No Maintenance Intended](http://unmaintained.tech/badge.svg)](http://unmaintained.tech)
[![Rust: nightly](https://img.shields.io/badge/Rust-nightly-blue.svg)](https://www.rust-lang.org)
[![License: (Apache-2.0 OR MIT)](https://img.shields.io/badge/LICENSE-Apache--2.0%20OR%20MIT-orange)](LICENSE-APACHE)
[![CI](https://github.com/offscale/cdd-rust/actions/workflows/ci-cargo.yml/badge.svg)](https://github.com/offscale/cdd-rust/actions/workflows/ci-cargo.yml)

Update: future of this repo remains unclear. Probably restart, delete all [non `.git`] files|folders and do a `cargo init`. One direction—different to the [Python](https://github.com/offscale/cdd-python) and [C](https://github.com/SamuelMarks/cdd-c) [cdd](https://compilers.com.au) implementations—is to go from:

0. SQL to [diesel](https://diesel.rs) ([`diesel::table!`](https://docs.rs/diesel/latest/diesel/macro.table.html));
1. [`diesel::table!`](https://docs.rs/diesel/latest/diesel/macro.table.html) to diesel `struct`s and functions (https://github.com/SamuelMarks/dsync); then
2. Diesel `struct`s to/fro [JSON-schema](https://json-schema.org/specification) via a new custom parser/emitter (compiler) written with https://github.com/rust-lang/rust-analyzer

Next, realise [JSON-schema](https://json-schema.org/specification) as part of a greater [OpenAPI](https://spec.openapis.org/oas/latest.html), and implement custom [rust-analyzer](https://github.com/rust-lang/rust-analyzer) parser/emitter code to go from/to:

3. [actix](https://actix.rs);
4. tests & mocks;
5. OpenAPI [dynamic: [utoipa](https://github.com/juhaku/utoipa); static: custom].

---

OpenAPI ↔ Rust. Compiler Driven Development (CDD) is a new development methodology, with implementations in many languages.

The central idea is to statically code-generate from target language to OpenAPI, and from OpenAPI back to target language.
All without having an untouchable 'generated' directory and without requiring `#[openapi]` annotations on `struct`s and routes.

Key other advantages are:

  - automated updating of tests and docs, making it feasible to maintain 100% coverage without trading off development agility;
  - synchronisation across language boundaries (e.g., between the frontends, and from them to the backend).

Longer-term there are many other advantages, including:

  - inversion of control, enabling the business analyst to design schemas (Google Forms or even MS Access style);
  - simplifying separating projects out into multiple smaller projects, and smaller projects into a big project;
  - providing an alternative to NoSQL for many user-defined schema scenarios (such as a survey-builder site).

---

## Developer guide

Install the latest version of [Rust](https://www.rust-lang.org). We tend to use nightly versions. [CLI tool for installing Rust](https://rustup.rs).

We use [rust-clippy](https://github.com/rust-lang-nursery/rust-clippy) linters to improve code quality.

There are plenty of [IDEs](https://areweideyet.com) and other [Rust development tools to consider](https://github.com/rust-unofficial/awesome-rust#development-tools).

### Step-by-step guide

```bash
# Install Rust (nightly)
$ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain nightly
# Install cargo-make (cross-platform feature-rich reimplementation of Make)
$ cargo install --force cargo-make
# Install rustfmt (Rust formatter)
$ rustup component add rustfmt
# Clone this repo
$ git clone https://github.com/offscale/cdd-rust && cd cdd-rust
# Run tests
$ cargo test
# Format, build and test
$ cargo make
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
