# Automating Client Library Updates

When using `cdd-rust` to build an SDK, keeping the generated client code synchronized with the source OpenAPI specification is crucial. The optimal strategy relies on a GitHub Actions cronjob.

## Updating the Generated Client SDK via GitHub Actions

This workflow fetches the latest OpenAPI specification and regenerates the Rust client SDK automatically.

Create a `.github/workflows/update-sdk.yml` file in your client SDK repository:

```yaml
name: Update SDK from OpenAPI

on:
  schedule:
    - cron: '0 0 * * *' # Run daily at midnight
  workflow_dispatch: # Allow manual triggering

jobs:
  update-sdk:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout SDK Repository
        uses: actions/checkout@v3

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: stable
          override: true

      - name: Install cdd-rust
        run: cargo install cdd-cli

      - name: Fetch OpenAPI Spec
        run: curl -O https://api.example.com/openapi.json

      - name: Generate SDK Code
        run: cdd-rust from_openapi to_sdk_cli -i openapi.json -o .

      - name: Check for Changes
        id: git-check
        run: |
          git status --porcelain
          echo "changed=$(git status --porcelain | wc -l)" >> $GITHUB_OUTPUT

      - name: Commit and Push Updates
        if: steps.git-check.outputs.changed > 0
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"
          git add .
          git commit -m "Auto-update SDK from OpenAPI spec"
          git push origin main
```

## Automating Crate Publishing

You can automate publishing this generated SDK to crates.io on a tag push. Add this to your `.github/workflows/publish.yml`:

```yaml
name: Publish SDK to crates.io

on:
  push:
    tags:
      - 'v*'

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          profile: minimal
          toolchain: stable

      - name: Publish to crates.io
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: cargo publish
```
