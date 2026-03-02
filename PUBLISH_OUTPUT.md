# Publishing SDKs

When `cdd-rust` generates an SDK (e.g., via `cdd-rust from_openapi to_sdk`), you can publish the generated Rust crate to Crates.io.

## Automated Updates

You can set up a GitHub Action cron job to periodically fetch the latest OpenAPI specification from your server, run `cdd-rust from_openapi to_sdk`, and push the changes if they differ.

```yaml
name: Update SDK
on:
  schedule:
    - cron: '0 0 * * *'
jobs:
  update:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - run: curl -O https://api.myserver.com/openapi.json
      - run: cdd-rust from_openapi to_sdk -i openapi.json -o .
      - run: |
          git config user.name "Bot"
          git config user.email "bot@example.com"
          git add .
          git commit -m "chore: update sdk" || echo "No changes"
          git push
```