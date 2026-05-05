Status: ready
Branch: fix-dts-private-method-destructure
Owner: Codex
Started: 2026-05-05

## Intent

Fix declaration emit for private methods with destructured parameters, targeting
`noImplicitAnyDestructuringInPrivateMethod`.

## Scope

- Keep dependency tracking for computed private method names.
- Skip parameter, type parameter, and return type dependency tracking for private
  methods because their signatures are omitted from `.d.ts` output.
- Prevent aliases used only by omitted private method signatures from forcing a
  local declaration and `export {};` marker.

## Verification

- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo fmt --package tsz-emitter -- --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo clippy -p tsz-emitter --lib -- -D warnings`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target ./scripts/emit/run.sh --dts-only --filter=noImplicitAnyDestructuringInPrivateMethod --verbose --concurrency=1 --timeout=30000 --json-out=/tmp/tsz-dts-private-method-destructure-rebased.json`
