# Developing cdd-rust

## Prerequisites

- Rust 1.75 or later
- Cargo
- `make`

## Building

Run `make build` to build the CLI and libraries.

## Testing

Run `make test` to execute the full test suite.
We enforce 100% test coverage and 100% documentation coverage.

## Project Structure

- `core/`: Parsing, emitting, and the intermediate representation logic.
- `cli/`: The command-line interface logic and JSON-RPC server.
- `web/`: Example models and routes.
