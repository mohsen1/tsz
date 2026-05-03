# fix(checker): keep distinct JSDoc @typedef alias names when bodies intern to the same shape

- **Date**: 2026-05-03
- **Branch**: `fix/jsdoc-typedef-distinct-alias-names`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance fingerprint parity (JSDoc display)

## Intent

Two JSDoc `@typedef` declarations whose body types intern to the same
structural shape (e.g. `A = { value?: number }` via `@property` and
`B = { value?: number }` inline) shared a single `DefId` because
`ensure_jsdoc_typedef_def` reused the body-matched alias regardless of
name. The display alias was set to whichever name registered first, so
TS2375 messages for assignments to `B` displayed `'A'` instead of `'B'`.

The fix only reuses the body-matched DefId when its name matches; otherwise
it falls back to the name-then-body lookup and registers a fresh DefId
for the new alias. Each typedef now keeps its own diagnostic display name.

## Files Touched

- `crates/tsz-checker/src/jsdoc/resolution/type_construction.rs` (~20 LOC change)
- `crates/tsz-checker/src/tests/jsdoc_typedef_distinct_alias_names_tests.rs` (new, ~95 LOC)
- `crates/tsz-checker/src/lib.rs` (+3 LOC: register the new test module)

## Verification

- `cargo nextest run -p tsz-checker --lib` — 3198 / 3198 passing
- `./scripts/conformance/conformance.sh run --filter "strictOptionalProperties3"` — 1 / 1 passing
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` — net +3 (strictOptionalProperties3 + typeFromParamTagForFunction + declarationEmitCommonJsModuleReferencedType flip FAIL → PASS)
