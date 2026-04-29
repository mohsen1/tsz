# fix(core): anchor inherited TS5102 at child's `compilerOptions` key

- **Date**: 2026-04-29
- **Branch**: `fix/core-ts5102-anchor-at-child-compiler-options-key2`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance and Fingerprints)

## Intent

Fix `verbatimModuleSyntaxCompat3.ts` fingerprint-only TS5102 anchor.

## Files Touched

- `crates/tsz-core/src/config/mod.rs` — change inherited TS5102 anchor
  from `"verbatimModuleSyntax"` to the child's `"compilerOptions"` key,
  plus a tempdir-based unit test.

## Verification

- `cargo nextest run -p tsz-core --lib -- test_ts5102_inherited` (1/1)
- `./scripts/conformance/conformance.sh run --filter "verbatimModuleSyntaxCompat"`
  → 4/4 PASS
- 2992/2992 tsz-core lib tests pass (20 skipped)
