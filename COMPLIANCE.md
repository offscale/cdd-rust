# OpenAPI 3.2.0 Compliance

`cdd-rust` has achieved **100% compliance** with the features introduced in the OpenAPI 3.2.0 specification. It features a highly compliant custom parser found in `core/src/oas`.

## Supported Workflows

- **Codebase ➔ OpenAPI:** Partially supported natively. The CLI automatically prepares database models for OpenAPI generation (`sync`) and can generate OpenAPI schemas for standalone Rust structs (`schema-gen`). The full OpenAPI routing document is usually built at compile time via ecosystem crates like `utoipa`.
- **OpenAPI ➔ Codebase (Scaffold New):** Fully supported. The `scaffold` command can generate all necessary modules, handler functions, payloads, and `.service()` registrations. The `test-gen` command writes integration tests.
- **OpenAPI ➔ Codebase (Merge Existing):** Fully supported via AST-aware patching. `cdd-rust` safely merges new routes and modifications without overwriting existing handwritten code.

## Implementation Details

* **Versions:** Supports OpenAPI 3.0, 3.1, and 3.2 directly.
* **Compatibility:** Implements shims for **OpenAPI 3.2**, specifically handling the `$self` keyword for Base URI
  determination (Appendix F) and downgrading version strings for library compatibility if needed.
* **Relative `$self`:** Resolves relative `$self` values (including dot-segment normalization) when matching local
  `$ref` targets and `operationRef` pointers.
* **Validation:** Enforces required `info`, URI/email formatting for Info/Contact/License, leading-slash `paths`,
  unique `operationId` values, templated path conflicts, component key naming rules, response status code keys,
  security scheme definitions, security requirement resolution, non-empty `response.description` and `requestBody.content`,
  mutual exclusivity of `example` vs `examples` for parameters/headers, sequential-only use of `itemSchema`,
  and rejects `additionalOperations` that reuse reserved HTTP methods.
* **Resolution:** Local `$ref` resolution plus base-URI-aware absolute/relative self-references (no external fetch).
* **Multi-Document `$ref`:** Optional `DocumentRegistry` resolves external OpenAPI/Schema documents (path items,
  parameters, request bodies, responses, headers, links, media types, and security schemes) plus schema
  `$id`/`$anchor` targets.
* **Schema `$id` References:** Resolves schema `$ref` values that match component or inline `$id`
  URIs (absolute or resolved against `$self`).
* **Schema Anchors:** Resolves `$anchor` / `$dynamicAnchor` targets for component or inline
  schemas and resolves `$dynamicRef` using dynamic anchor scope (falling back to standard `$ref` resolution).
* **Relative Server URLs:** Resolves relative `servers.url` values (e.g., `.`, `./v1`, `v1`) into base paths with RFC3986 dot-segment normalization, using the retrieval URI when provided.
* **Polymorphism:** handles `oneOf`, `anyOf`, and `allOf` (flattening) into Rust Enums and Structs.
* **Discriminator Defaults:** supports `discriminator.defaultMapping` for OAS 3.2 polymorphic schemas.
* **Extractors:** Maps OAS parameters to backend-specific extractors (e.g., `web::Query`, `web::Path`, `web::Json`,
  `SecurityScheme`).
* **Media Types:** Recognizes vendor `+json` media types, `text/*`, and binary request bodies with dedicated extractors.
* **Media Type References:** Resolves `components.mediaTypes` `$ref` entries inside `content` (request bodies, responses, and headers).
* **Parameter Content Media Types:** Resolves `components.mediaTypes` `$ref` in parameter `content`, supports `itemSchema` for sequential media types, and honors `serializedValue`/`externalValue` examples.
* **Content Schema Mapping:** Uses `contentSchema` (for JSON-encoded string payloads) to drive strong typing for parameters and request bodies.
* **Sequential Multipart:** supports `multipart/mixed` and `multipart/byteranges` `itemSchema` normalization.
* **Positional Encoding:** parses `prefixEncoding` / `itemEncoding` for multipart media types (OAS 3.2).
* **Nested Encoding:** parses and emits nested Encoding Object `encoding` / `prefixEncoding` / `itemEncoding` fields.
* **Examples:** Uses parameter `content` examples when generating contract tests, and honors
  `dataValue` / `serializedValue` / `externalValue` for parameters and request bodies (serialized examples bypass re-encoding).
* **Sequential Media Types:** Supports `itemSchema` for sequential JSON request bodies (e.g., `jsonl`, `ndjson`) and maps to `Vec<T>`.
* **Sequential Response Types:** Uses `itemSchema` for sequential media types (including `text/event-stream` and `multipart/*`) to infer `Vec<T>` when no `schema` is present.
* **Sequential Vendor Suffixes:** Treats `+jsonl`, `+ndjson`, and `+json-seq` media types as sequential for `itemSchema` typing.
* **Response Validation:** Contract tests validate JSON, vendor `+json`, sequential JSON, and `text/event-stream` responses.
* **Response Headers:** Resolves response header `$ref` and `content` definitions when extracting response metadata.
* **Response Header Content Priority:** Prefers `content` (or preserved `x-cdd-content`) over injected `schema` when parsing
  response header media types, ensuring `content`-based headers round-trip correctly.
* **Response Header Content Round-Trip:** Preserves header `content` media types (and examples) when generating OpenAPI.
* **Response Summary:** Preserves Response Object `summary` during parsing and OpenAPI generation (including callbacks).
* **Schema Dialects:** Contract tests select JSON Schema draft via `jsonSchemaDialect` or per-schema `$schema` (defaults to Draft 2020-12 for OAS 3.1/3.2, Draft 4 for OAS 3.0/Swagger 2.0).
* **Header Arrays/Objects:** Contract tests validate `schema`-based header arrays/objects using `style: simple` (explode and non-explode).
* **Reference Overrides:** Honors Reference Object `description` overrides for responses during validation and resolution.
* **Set-Cookie Handling:** Validates `Set-Cookie` headers without comma-splitting, preserving cookie values that contain commas.
* **Querystring:** Serializes `querystring` params
  as JSON when the media type is `application/json` (RFC3986-encoded).
* **Querystring Extractors:** Non-form querystring parameters are surfaced as raw `String` values
  in generated handlers to avoid incorrect form decoding; form-encoded querystrings remain typed.
* **Header/Cookie Params:** Contract tests serialize header/cookie parameters using OAS `style`/`explode` rules or `content` media types.
* **Header Validation:** Enforces Header Object constraints (`schema` vs `content`, `style: simple`, no `allowEmptyValue`), and ignores `Content-Type` headers.
* **Media Type Examples:** Validates `example` vs `examples` mutual exclusivity in request/response content.
* **Component Media Types:** Validates `components.mediaTypes` example conflicts and encoding field compatibility.
* **Serialized/External Examples:** Preserves `serializedValue` and `externalValue` examples when generating OpenAPI for parameters and request bodies.
* **Example Metadata:** Preserves Example Object `summary` and `description` when round-tripping examples.
* **Example Ref Overrides:** Applies Reference Object `summary`/`description` overrides when resolving Example `$ref` values.
* **Boolean Schemas:** Handles `schema: true/false` in request/response bodies and response headers (rejects required `false` bodies).
* **Nullable Normalization:** Converts `nullable` / `x-nullable` into `type: [..., "null"]` for OpenAPI → Rust typing.
* **Byte/Binary Formats:** Maps `format: byte` / `format: binary` to `Vec<u8>` in OpenAPI → Rust type mapping.
* **Boolean Parameter Schemas:** Accepts `schema: true` for parameters (maps to `String`) and rejects `schema: false`.
* **Example Precedence:** Honors `serializedValue` / `externalValue` examples for parameters even when `value` is present.
* **Style Validation:** Enforces type constraints for `deepObject`, `spaceDelimited`, and `pipeDelimited` parameter styles.
* **Schema ExternalDocs:** emits `externalDocs` metadata when generating OpenAPI schemas from Rust models.
* **Schema ExternalDocs Validation:** Validates `externalDocs.url` on schema objects (including nested schemas).
* **Schema XML Validation:** Validates schema `xml` objects, including `nodeType` values and conflicts with deprecated
  `attribute` / `wrapped` fields.
* **Serde Mapping:** respects `rename_all` and `deny_unknown_fields` when generating OpenAPI schemas from Rust models.
* **Link Servers:** Applies Link Object `server` overrides (including defaulted variables) when generating HATEOAS link construction code.
* **Link Server Metadata:** Preserves Link Object `server` details (name/description/variables) when generating OpenAPI.
* **Link Validation:** Resolves `operationRef` pointers to concrete path+method targets and errors on unknown `operationId` links.
* **Link Components:** Resolves `operationRef` pointers to `components.pathItems` when the component is referenced by a
  unique path (errors on ambiguity).
* **Link Round-Trip:** Resolves `operationId`/`operationRef` targets for codegen without mutating the original Link Object,
  avoiding invalid `operationId`+`operationRef` emissions during OpenAPI generation.
* **Link Validation (Local Ref):** Errors when a local `operationRef` fails to resolve to a known operation.
* **Link Object Validation:** Enforces exactly one of `operationId` or `operationRef`, validates Link Object `server` definitions, enforces link name key patterns, and detects link `$ref` cycles.
* **Link Parameter Keys:** Validates qualified Link `parameters` keys (e.g., `path.id`) and substitutes them into link templates using the unqualified parameter name.
* **Link Parsing:** Accepts normalized snake_case link keys (`operation_id`, `operation_ref`, `request_body`) during YAML preprocessing.
* **OAuth2/OIDC Schemes:** Preserves OAuth2 flows (including device authorization) and OpenID Connect discovery URLs when generating OpenAPI.
* **Contract Tests:** Skips webhook routes (inbound) during test generation.
* **Contract Tests (Multipart):** Builds multipart request payloads from example values and per-part `contentType` hints.
* **Contract Tests (Encoding Headers):** Applies `Encoding` object headers to multipart parts (excluding `Content-Type`) and respects `Encoding.contentType` for JSON-form-urlencoded fields.
* **Encoding Header Refs:** Resolves `Encoding.headers` `$ref` entries against `components.headers` when extracting request body definitions.
* **Callbacks:** Enforces `operationId` uniqueness across callbacks and top-level operations.
* **Callback Parameters:** Parses callback Path Item and Operation parameters, merging them for callback operations and emitting them during OpenAPI generation.
* **Top-Level Security:** Preserves root-level `security` requirements when generating OpenAPI documents.
* **Operation Security Overrides:** Preserves explicit operation-level `security` (including empty arrays) without
  leaking global security into each operation during OpenAPI round-trips.
* **Callback Security:** Parses and emits callback operation `security` requirements (including inherited defaults).
* **Header Parameter Case:** Treats header parameter names as case-insensitive when detecting duplicates.
* **OpenAPI Generation:** Emits top-level `tags` and `servers` (or per-operation `servers`) from parsed routes.
* **Round-Trip Metadata:** Preserves original `operationId` values and selected response status/description/media type in generated OpenAPI (including callbacks).
* **Document Metadata Round-Trip:** `parse_openapi_document` extracts `$self`, `jsonSchemaDialect`, `info`, `servers`, `tags`, `externalDocs`, and root-level `x-` extensions for OpenAPI → Rust → OpenAPI workflows.
* **Component Preservation:** Can merge existing `components` (responses, examples, mediaTypes, etc.) when generating OpenAPI.
* **Schema Keyword Passthrough:** Preserves advanced JSON Schema keywords (`if/then/else`, `dependentSchemas`, `unevaluatedProperties`, `contentSchema`) across parameters, request/response bodies, and component schemas during OpenAPI ↔ Rust round-trips.
* **Paths/Webhooks Extensions:** Accepts `x-` keys at the `paths` and `webhooks` object level without treating them as path items,
  and preserves them during OpenAPI generation.
* **Path Item Extensions:** Preserves `x-` extensions on Path Item Objects during OpenAPI ↔ Rust round-trips.
* **Server Override Round-Trip:** Preserves path/operation-level `servers` objects (including variables) instead of collapsing to base paths.
* **Server URL Validation:** Rejects invalid URI references in `servers.url` (including whitespace in literal segments).
