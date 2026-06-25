# CDD Web Mock Server

This is the generated mock server implementation. It supports multiple, decoupled execution modes to facilitate UI development, automated testing, and production simulation.

## Execution Modes

- `cargo run` (No DB configured): **Stub Mode**. Server runs using traditional scaffolds, endpoints return `NotImplementedError` (501) or empty bodies.
- `cargo run` (With `DATABASE_URL`): **Production Mode**. Uses actual ORM interactions against a real database.
- `cargo run -- --ephemeral`: **Sandbox Mode**. Uses actual ORM interactions against a fresh, throwaway database (in-memory or temporary schema).
- `cargo run -- --ephemeral --seed`: **Full Mock Mode**. Ephemeral database, automatically populated with a localized fake data relational graph.

## Advanced Capabilities

### Strict Request Validation

Enable strict OpenAPI schema validation on incoming requests:
```bash
cargo run -- --strict-validation
```

### Authentication Enforcement

Toggle mock authentication requirements. When enabled without a DB, the server validates against `Bearer mock-token-123`.
```bash
cargo run -- --enforce-auth
```

### Integrated Identity Provider

Start the integrated Auth Server to test login/registration flows natively:
```bash
cargo run -- --start-auth-server
```

## Administrative Webhooks

If the OpenAPI spec defines callbacks or webhooks, you can manually trigger them using the administrative endpoints:

```bash
curl -X POST "http://localhost:8080/_mock/trigger-webhook/{webhook_name}?target_url=http://your-receiver.com/hook"
```
