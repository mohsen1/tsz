Status: ready
Branch: fix-dts-private-name-error
Owner: Codex
Started: 2026-05-05

## Intent

Fix declaration emit for exported inferred class-return types that reference a private unique-symbol computed property name, targeting `declarationEmitPrivateNameCausesError`.

## Planned Scope

- Retain unique-symbol property-name dependencies discovered while walking inferred object types.
- Analyze returned class expressions in unannotated exported functions so computed property backing declarations survive public API pruning.
- Keep the required `export {};` scope marker when a private dependency declaration is emitted.

## Verification Plan

- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo fmt --package tsz-emitter -- --check`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo clippy -p tsz-emitter --lib -- -D warnings`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target ./scripts/emit/run.sh --dts-only --filter=declarationEmitPrivateNameCausesError --verbose --concurrency=1 --timeout=30000 --json-out=/tmp/tsz-dts-private-name-final.json`
