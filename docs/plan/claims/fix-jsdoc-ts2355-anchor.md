# fix(checker): anchor TS2355 on JSDoc return-type token for JS function declarations

- **Date**: 2026-04-26
- **Branch**: `fix/jsdoc-ts2355-anchor`
- **PR**: #1431
- **Status**: ready
- **Workstream**: Conformance fingerprint parity

## Intent

`conformance/jsdoc/jsdocFunction_missingReturn.ts` was a fingerprint-only
failure: tsc anchors TS2355 on the JSDoc return-type token (`number` inside
`@type {function(): number}`) while we anchored on the function name. This
PR adds a JSDoc lookup helper that resolves the source span of the JSDoc
return-type token for a function and threads it into the TS2355/TS2366
emission paths in `function_declaration_checks.rs`. When the helper resolves
a span, the diagnostic uses that span; otherwise the legacy fall-back to
function name / function node is preserved.

## Files Touched

- `crates/tsz-checker/src/jsdoc/lookup.rs` (~66 LOC new helper
  `jsdoc_function_return_type_span_for_function`).
- `crates/tsz-checker/src/state/state_checking_members/function_declaration_checks.rs`
  (~60 LOC change: thread `has_jsdoc_return_type` into
  `check_function_return_paths`, prefer the JSDoc span on the
  `requires_return && falls_through` and `undefined | T` TS2355 branches).
- `crates/tsz-checker/src/lib.rs` (~3 LOC test registration).
- `crates/tsz-checker/tests/jsdoc_function_return_type_anchor_tests.rs`
  (~67 LOC, two regression tests: pinned anchor + no-JSDoc fall-back).

## Verification

- `cargo nextest run -p tsz-checker -E 'test(jsdoc_function_return_type_anchor)'`
  (2/2 passed).
- `cargo nextest run -p tsz-checker --lib` (2891 tests pass).
- `./scripts/conformance/conformance.sh run --filter "jsdocFunction_missingReturn"`
  flips to PASS (was fingerprint-only).
- `./scripts/conformance/conformance.sh run --filter "jsdoc"` 359/377 (95.2%)
  vs baseline 358/377 (95.0%): +1 fix, no regressions.
