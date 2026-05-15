# Compliance

`cdd-rust` is fully compliant with the following standards:
- **OpenAPI Specification**: 3.2.0 (Full Support)
- **JSON Schema Dialect**: 2020-12
- **Rust Edition**: 2021

## Conformance Verification

The toolchain has been fully verified against the universal conformance suite via `../scripts/check_all_conformance.sh`. All features, including callbacks, links, and examples, are successfully roundtripped and marked as green in the `ECOSYSTEM_COMPLIANCE_OAS_3_2_0.md` tracking document.