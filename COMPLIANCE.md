# Compliance

`cdd-rust` is fully compliant with the following standards:
- **OpenAPI Specification**: 3.2.0 (Full Support) & Swagger 2.0 (Automatic Upgrade & Full Support)
- **Arazzo Specification**: 1.1.0 (Full Support)
- **Model Context Protocol (MCP)**: 1.0.0 (`stdio` Server & Generated Client/Server Adapters)
- **JSON Schema Dialect**: 2020-12
- **Rust Edition**: 2021

## Conformance Verification

The toolchain has been verified against the universal conformance suite via `../scripts/check_all_conformance.sh`. All features—including callbacks, webhooks, links, runtime expressions, and examples—are roundtripped and verified in [`conformance/`](conformance/).

## OpenAPI 3.2.0 & Swagger 2.0 Support Matrix

| Specification Concept       | Status |
| --------------------------- | ------ |
| Info Object                 | ✅     |
| Server Object               | ✅     |
| Components Object           | ✅     |
| Paths Object                | ✅     |
| Path Item Object            | ✅     |
| Operation Object            | ✅     |
| Parameter Object            | ✅     |
| Request Body Object         | ✅     |
| Responses Object            | ✅     |
| Responses / Schema Object   | ✅     |
| Reference Object (`$ref`)   | ✅     |
| OAuth Flows                 | ✅     |
| Security Requirement Object | ✅     |
| Links / Callbacks           | ✅     |
| Webhooks                    | ✅     |

## Arazzo 1.1.0 Support Matrix

| Arazzo 1.1.0 Concept                     | Status |
| ---------------------------------------- | ------ |
| Arazzo Object (`arazzo`, `$self`)        | ✅     |
| Info Object                              | ✅     |
| Source Description Object (`openapi`, `asyncapi`, `arazzo`) | ✅     |
| Workflow Object (`dependsOn`, `inputs`, `outputs`) | ✅     |
| Step Object (`operationId`, `operationPath`, `channelPath`, `workflowId`, `timeout`, `dependsOn`) | ✅     |
| Parameter Object & Reusable Object       | ✅     |
| RequestBody & Payload Replacement Object | ✅     |
| Criterion & Criterion Expression Type    | ✅     |
| Selector Object                          | ✅     |
| Success Action & Failure Action Objects  | ✅     |
| Components Object                        | ✅     |
| Runtime Expressions (All 17 Roots)       | ✅     |
