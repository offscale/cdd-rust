.PHONY: install_base install_deps build_docs build test run help all build_wasm build_docker run_docker

# If the first argument is "run", we can pass the rest of the arguments to the target
ifeq (run,$(firstword $(MAKECMDGOALS)))
  RUN_ARGS := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))
  $(eval $(RUN_ARGS):;@:)
endif

all: help

help:
	@echo "Available commands:"
	@echo "  install_base   - Install Rust and required targets"
	@echo "  install_deps   - Install dependencies"
	@echo "  build_docs     - Build the API docs. Use DOC_DIR=<path> to specify alternative directory."
	@echo "  build          - Build the CLI binary. Use BIN_DIR=<path> to specify alternative directory."
	@echo "  test           - Run the test suite."
	@echo "  run            - Run the CLI tool."
	@echo "  build_wasm     - Build the WASM target (Note: currently unsupported)"
	@echo "  build_docker   - Build alpine and debian Docker images."
	@echo "  run_docker     - Run the built Docker images to test them."

install_base:
	rustup update
	rustup target add wasm32-unknown-unknown || true
	rustup target add wasm32-unknown-emscripten || true

install_deps:
	cargo fetch

DOC_DIR ?= target/doc
build_docs:
	cargo doc --no-deps --target-dir $(DOC_DIR)

BIN_DIR ?= target/release
build:
	cargo build --release -p cdd-cli
	@if [ "$(BIN_DIR)" != "target/release" ]; then \
		mkdir -p $(BIN_DIR); \
		cp target/release/cdd-rust $(BIN_DIR)/; \
	fi

test:
	cargo test --workspace

run: build
	./target/release/cdd-rust $(RUN_ARGS)

build_wasm:
        @echo "Building WASM target..."
        rustup target add wasm32-wasip1
        cargo build -p cdd-cli --release --target wasm32-wasip1 --no-default-features
build_docker:
	docker build -t cdd-rust:alpine -f alpine.Dockerfile .
	docker build -t cdd-rust:debian -f debian.Dockerfile .

run_docker:
	@echo "Testing Alpine image..."
	docker run --rm -d --name cdd_alpine -p 8082:8082 cdd-rust:alpine
	sleep 2
	curl -X POST -H "Content-Type: application/json" -d '{"jsonrpc": "2.0", "method": "version", "id": 1}' http://localhost:8082 || true
	docker stop cdd_alpine || true
	@echo "Testing Debian image..."
	docker run --rm -d --name cdd_debian -p 8082:8082 cdd-rust:debian
	sleep 2
	curl -X POST -H "Content-Type: application/json" -d '{"jsonrpc": "2.0", "method": "version", "id": 1}' http://localhost:8082 || true
	docker stop cdd_debian || true
