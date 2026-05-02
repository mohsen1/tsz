Status: claim
Branch: fix/dts-import-type-generic-arrow
Owner: Codex
Started: 2026-05-02

## Intent

Fix declaration emit for generic arrow/function expressions wrapped by imported generic return types, targeting `importTypeGenericArrowTypeParenthesized`.

## Planned Scope

- Declaration emitter type inference / type formatting around imported generic aliases.
- Focused regression coverage for the emitted `.d.ts` shape.

## Verification Plan

- `cargo nextest run -p tsz-emitter <focused-test>`
- `TSZ_BIN=/tmp/tsz-tail-failures/.target/release/tsz scripts/safe-run.sh ./scripts/emit/run.sh --dts-only --filter=importTypeGenericArrowTypeParenthesized --verbose --json-out=/tmp/tsz-tail-failures/.tmp-import-type-generic-arrow.json`
