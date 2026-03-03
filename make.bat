@ECHO OFF
SETLOCAL ENABLEDELAYEDEXPANSION

IF "%1"=="" GOTO help
IF "%1"=="help" GOTO help
IF "%1"=="all" GOTO help
IF "%1"=="install_base" GOTO install_base
IF "%1"=="install_deps" GOTO install_deps
IF "%1"=="build_docs" GOTO build_docs
IF "%1"=="build" GOTO build
IF "%1"=="test" GOTO test
IF "%1"=="run" GOTO run
IF "%1"=="build_wasm" GOTO build_wasm
IF "%1"=="build_docker" GOTO build_docker
IF "%1"=="run_docker" GOTO run_docker

ECHO Unknown command: %1
GOTO help

:help
ECHO Available commands:
ECHO   install_base   - Install Rust and WASM targets
ECHO   install_deps   - Fetch dependencies
ECHO   build_docs     - Build API docs (Use DOC_DIR to override path)
ECHO   build          - Build CLI binary (Use BIN_DIR to override path)
ECHO   test           - Run tests
ECHO   run            - Run the CLI tool (e.g., make.bat run --version)
ECHO   build_wasm     - Build the WASM target
ECHO   build_docker   - Build Alpine and Debian images
ECHO   run_docker     - Run built Docker images to test them
GOTO :EOF

:install_base
rustup update
rustup target add wasm32-unknown-unknown
GOTO :EOF

:install_deps
cargo fetch
GOTO :EOF

:build_docs
IF "%DOC_DIR%"=="" SET DOC_DIR=target\doc
cargo doc --no-deps --target-dir %DOC_DIR%
GOTO :EOF

:build
IF "%BIN_DIR%"=="" SET BIN_DIR=target\release
cargo build --release
IF NOT "%BIN_DIR%"=="target\release" (
    IF NOT EXIST "%BIN_DIR%" MKDIR "%BIN_DIR%"
    COPY target\release\cdd-rust.exe "%BIN_DIR%\"
)
GOTO :EOF

:test
cargo test
GOTO :EOF

:run
CALL :build
SHIFT
SET RUN_ARGS=
:loop
IF "%1"=="" GOTO end_loop
SET RUN_ARGS=%RUN_ARGS% %1
SHIFT
GOTO loop
:end_loop
target\release\cdd-rust.exe %RUN_ARGS%
GOTO :EOF

:build_wasm
ECHO Attempting WASM build. See WASM.md for current limitations.
cargo build -p cdd-cli --target wasm32-unknown-unknown --release || ECHO WASM build is currently unsupported. See WASM.md for details.
GOTO :EOF

:build_docker
docker build -t cdd-rust:alpine -f alpine.Dockerfile .
docker build -t cdd-rust:debian -f debian.Dockerfile .
GOTO :EOF

:run_docker
ECHO Testing Alpine image...
docker run --rm -d --name cdd_alpine -p 8082:8082 cdd-rust:alpine
timeout /T 2
curl -X POST -H "Content-Type: application/json" -d "{\"jsonrpc\": \"2.0\", \"method\": \"version\", \"id\": 1}" http://localhost:8082 || VER>NUL
docker stop cdd_alpine

ECHO Testing Debian image...
docker run --rm -d --name cdd_debian -p 8082:8082 cdd-rust:debian
timeout /T 2
curl -X POST -H "Content-Type: application/json" -d "{\"jsonrpc\": \"2.0\", \"method\": \"version\", \"id\": 1}" http://localhost:8082 || VER>NUL
docker stop cdd_debian
GOTO :EOF
