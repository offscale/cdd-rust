# Task: Interpolate `CDD_TEMPLATE_README.md` and Audit Root Markdown Documentation

Update this repository's `README.md` (and synchronize any related root markdown files such as `USAGE.md`, `COMPLIANCE.md`, `ARCHITECTURE.md`, `DEVELOPING.md`, `WASM.md`, and `PUBLISH.md` if discrepancies are found) by interpolating the canonical template at `../cdd-openapi-test-harness/CDD_TEMPLATE_README.md` (or `/Users/samuel/repos/cdd-openapi-test-harness/CDD_TEMPLATE_README.md`).

Do **not** alter the shared structure, prose, Mermaid diagram, or polyglot ecosystem table from `CDD_TEMPLATE_README.md` except where replacing the template variables listed below (and adjusting the `# ====` underline on line 2 to match the length of line 1).

---

## 1. Template Variables to Interpolate

Every occurrence of the following template variables in `CDD_TEMPLATE_README.md` must be replaced with accurate, repository-specific content:

1. **`{{PROJECT_NAME}}`**
   - The exact repository / CLI binary name (e.g., `cdd-c`, `cdd-cpp`, `cdd-csharp`, `cdd-go`, `cdd-java`, `cdd-kotlin`, `cdd-php`, `cdd-python-all`, `cdd-ruby`, `cdd-rust`, `cdd-sh`, `cdd-swift`, `cdd-ts`).
   - *Note*: Adjust the `====` underline on line 2 so its character count matches `{{PROJECT_NAME}}: {{LANGUAGE}} ⇄ OpenAPI` on line 1.

2. **`{{LANGUAGE}}`**
   - The human-readable language name for this toolchain (e.g., `C`, `C++`, `C#`, `Go`, `Java`, `Kotlin`, `PHP`, `Python`, `Ruby`, `Rust`, `POSIX Shell`, `Swift`, `TypeScript`).

3. **`{{GITHUB_ORG}}`**
   - The GitHub organization or user hosting this repository (`SamuelMarks` or `offscale`—verify against the Global Polyglot Ecosystem table in `CDD_TEMPLATE_README.md` or `git remote -v`).

4. **`{{SDK_EXAMPLE}}`**
   - A concise, idiomatic, and **verified** code block in `{{LANGUAGE}}` demonstrating how to import and invoke this repository's programmatic SDK (e.g., generating an SDK/server from `spec.json` or running a sync/parse call via the native API).
   - Verify the exact class, struct, function, and header/module names against the current codebase (`src/`, `include/`, `lib/`, `Sources/`, etc.) rather than copying stale examples.

5. **`{{INSTALLATION_AND_DEVELOPMENT}}`**
   - The native toolchain build, install, lint, and test commands for this language (e.g., `cmake` + `ctest`, `cargo build` + `cargo test`, `dotnet build` + `dotnet test`, `go build` + `go test`, `./gradlew build`, `mvn test`, `composer test`, `pytest`, `bundle exec rspec`, `swift build` + `swift test`, `npm install` + `npm test`), complementing the cross-platform `make` / `make.bat` block already in the template.

6. **`{{CLI_CORE_SUBCOMMANDS_HELP}}`**
   - The **exact, verbatim** `--help` output captured by actually executing the built CLI binary in this repository (see Section 2 below).

7. **`{{SPECIAL_FEATURES_BEYOND_CDD_SCOPE}}`**
   - A comprehensive, well-structured breakdown of all capabilities, subcommands, flags, frameworks, ORMs, transformers, UI generators, and runtime modes implemented in this repository beyond the common CDD baseline (see Section 3 below).

---

## 2. Capture Verbatim CLI `--help` Output (`{{CLI_CORE_SUBCOMMANDS_HELP}}`)

Do **not** guess or hand-write `--help` output. Build/run the CLI binary and execute `--help` for the root command and every subcommand:

1. **Root CLI Help**:
   - `<cli> --help`
2. **Core OpenAPI Subcommands**:
   - `<cli> from_openapi --help`
   - `<cli> from_openapi to_sdk --help`
   - `<cli> from_openapi to_sdk_cli --help`
   - `<cli> from_openapi to_server --help`
   - `<cli> to_openapi --help`
3. **Core Arazzo Subcommands**:
   - `<cli> from_arazzo --help`
   - `<cli> from_arazzo to_sdk --help` (if sub-target has separate help)
   - `<cli> from_arazzo to_sdk_cli --help` (if sub-target has separate help)
   - `<cli> from_arazzo to_server --help` (if sub-target has separate help)
   - `<cli> to_arazzo --help`
4. **Core Sync, Docs, JSON-RPC, and MCP Subcommands**:
   - `<cli> sync --help`
   - `<cli> to_docs_json --help`
   - `<cli> serve_json_rpc --help`
   - `<cli> mcp --help`
5. **Any Language-Specific Subcommands**:
   - Run `<cli> <extra_subcommand> --help` for every additional subcommand shown in the root `--help` output (e.g., `bind`, `audit`, `transformer`, `standardize-gnu`, `from_google_discovery`, `merge_swift`, `sync_arazzo`, `scaffold`, `test-gen`, `schema-gen`, `parse`, `emit`, etc.).

Format each help block under a `#### `<subcommand>`` heading with a fenced `text` or `console` code block containing the exact stdout output. If you discover while running `--help` that any required core subcommand or help text is missing or broken in the CLI implementation, fix the CLI help output first and re-run it so the documentation reflects working reality.

---

## 3. Audit the Codebase for `{{SPECIAL_FEATURES_BEYOND_CDD_SCOPE}}`

Before writing `{{SPECIAL_FEATURES_BEYOND_CDD_SCOPE}}`, perform a thorough audit of the repository's source tree, CLI argument parser, test suite, and existing markdown docs (`README.md`, `USAGE.md`, `ARCHITECTURE.md`, `CAPABILITIES.md`, `COMPLIANCE.md`, `WASM.md`):

1. **Inspect the CLI Entrypoint & Argument Parser**:
   - Identify all language-specific subcommands, flags, and environment variables (e.g., `--framework`, `--serverFramework`, `--implementation`, `--orm`, `--target`, `--admin`, `--ephemeral`, `--seed`, `--with-postgres`, `--with-faker`, `--wasi`, etc.).
2. **Inspect the Emitters, Parsers, and Transformers**:
   - Document target web/HTTP frameworks (e.g., Actix/Axum/Reqwest, Express/Bun/Deno, Angular/React/Vue, FastAPI, Ktor Multiplatform, Vapor).
   - Document ORM and database integrations (e.g., `c-orm` migrations, SQLAlchemy, Diesel, TypeORM, ActiveRecord, Fluent, Entity Framework, DAO stub/concrete/ephemeral/seeded SQLite modes, PostgreSQL `libpq`).
   - Document syntax tree engines and source-to-source refactoring tools (e.g., `LibCST`, `SwiftSyntax`, `Roslyn`, `JavaParser`, Go `dst`, C89 CST lowering, `bind` 42-language FFI generator, memory safety auditors).
   - Document extra specification or UI capabilities (e.g., Google Discovery `from_google_discovery`, Auto-Admin UI generation, in-memory JSON-RPC `process_in_memory`, WASI execution modes).
3. **Link to Companion Root Markdown Files**:
   - Ensure any specialized markdown files in the repository (e.g., `CAPABILITIES.md`, `DSL.md`, `TO_DOCS_JSON.md`) are referenced where relevant, and verify that `USAGE.md` and `COMPLIANCE.md` are up to date with the codebase.

---

## 4. Verification Checklist

Before finishing, verify that:
- [ ] Zero uninterpolated `{{...}}` placeholders remain in `README.md`.
- [ ] Every `--help` block in `README.md` matches the actual output of the CLI in this repository.
- [ ] The `{{SDK_EXAMPLE}}` uses real symbols/types that exist in this repository.
- [ ] All special features and extra CLI flags/subcommands present in the codebase are documented under `## Special Features Beyond the Common Compiler Driven Development Subset`.
