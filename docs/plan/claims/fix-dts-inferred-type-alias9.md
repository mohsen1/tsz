Status: ready
Branch: fix-dts-inferred-type-alias9
Owner: Codex
Started: 2026-05-05

## Intent

Fix declaration emit for exported functions whose inferred return type references
a non-exported top-level type alias, targeting `declarationEmitInferredTypeAlias9`.

## Scope

- Walk inferred return-expression dependencies for exported functions without an
  explicit return annotation.
- Preserve declared type annotations for returned identifiers so local aliases
  used by the public API survive declaration elision.
- Validate that module-mode emit keeps the alias and `export {};` marker.

## Verification

- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo fmt --package tsz-emitter -- --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo clippy -p tsz-emitter --lib -- -D warnings`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target ./scripts/emit/run.sh --dts-only --filter=declarationEmitInferredTypeAlias9 --verbose --concurrency=1 --timeout=30000 --json-out=/tmp/tsz-dts-inferred-type-alias9-rebased.json`
