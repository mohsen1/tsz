Status: ready
Branch: fix-dts-inferred-type-alias4
Owner: Codex
Started: 2026-05-05

## Intent

Fix declaration emit for function-local recursive inferred type aliases, targeting
`declarationEmitInferredTypeAlias4`.

## Scope

- Avoid preserving function-local type aliases as nameable declaration return type
  applications.
- Expand returned local annotations that reference function-local aliases through
  the inferred declaration printer.
- Elide recursive references that remain after expanding a function-local alias.

## Verification

- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo fmt --package tsz-emitter -- --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo clippy -p tsz-emitter --lib -- -D warnings`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target ./scripts/emit/run.sh --dts-only --filter=declarationEmitInferredTypeAlias4 --verbose --concurrency=1 --timeout=30000 --json-out=/tmp/tsz-dts-inferred-type-alias4-final2.json`
