# Compliance

`cdd-rust` is fully compliant with the following standards:
- **OpenAPI Specification**: 3.2.0 (Full Support)
- **Arazzo Specification**: 1.1.0 (Full Support)
- **JSON Schema Dialect**: 2020-12
- **Rust Edition**: 2021

## Conformance Verification

The toolchain has been fully verified against the universal conformance suite via `../scripts/check_all_conformance.sh`. All features, including callbacks, links, and examples, are successfully roundtripped and marked as green in the `ECOSYSTEM_COMPLIANCE_OAS_3_2_0.md` tracking document.
## Support Matrix

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
