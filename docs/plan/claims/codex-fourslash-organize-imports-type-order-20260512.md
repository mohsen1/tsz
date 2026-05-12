# fix(fourslash): honor organize imports type order

- **Date**: 2026-05-12
- **Branch**: `codex/fourslash-organize-imports-type-order-20260512`
- **PR**: #5752
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Restore `organizeImportsType8` by converting organize-imports edits to
`FileTextChanges` and honoring `organizeImportsTypeOrder` in named import
sorting. Local reproduction showed the same failure on current main, so this
fixes an independent fourslash blocker rather than a regression in #5739.

## Files Touched

- `crates/tsz-cli` organize-imports path
- `scripts/fourslash` verification outputs
- `docs/plan/claims/codex-fourslash-organize-imports-type-order-20260512.md`

## Verification

- `cargo fmt --all`
- `cargo test -p tsz-cli organize_imports_honors_type_order_preference -- --nocapture`
- `./scripts/fourslash/run-fourslash.sh --filter=organizeImportsType8 --workers=1 --timeout=60000 --json-out=/tmp/tsz-main-organizeImportsType8-fixed.json`
- `./scripts/fourslash/run-fourslash.sh --filter=organizeImportsType --workers=1 --timeout=60000 --json-out=/tmp/tsz-organizeImportsType-fixed.json`
- `./scripts/fourslash/run-fourslash.sh --shard=1/6 --shard-strategy=weighted --skip-build --workers=4 --timeout=60000 --json-out=/tmp/tsz-fourslash-shard1-after-organize.json`
