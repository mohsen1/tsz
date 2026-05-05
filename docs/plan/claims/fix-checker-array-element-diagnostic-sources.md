# fix(checker): widen array element diagnostic sources

- **Date**: 2026-05-05
- **Branch**: `fix/checker-array-element-diagnostic-sources`
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-05 10:25:28 UTC

## Intent

Fix the random conformance pick `destructuringParameterDeclaration1ES5.ts`,
where `tsc` and `tsz` emit the same TS2322/TS2345/TS2393/TS7006/TS7010/TS7031
codes but disagree on TS2322 source-type display for destructuring parameter
defaults. `tsc` reports widened primitives (`string`, `number`) for the
argument array elements; `tsz` currently reports literal types (`"string"`,
`1`, `2`).

## Files Touched

- `docs/plan/claims/fix-checker-array-element-diagnostic-sources.md`
- `crates/tsz-checker/src/error_reporter/assignability.rs`
- `crates/tsz-checker/src/error_reporter/assignability_helpers.rs`
- `crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs`
- `crates/tsz-checker/src/error_reporter/call_errors/elaboration_array_mismatch.rs`
- `crates/tsz-checker/src/error_reporter/call_errors_tests.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source/assignment_formatting.rs`
- `crates/tsz-checker/src/error_reporter/core/type_display.rs`
- `crates/tsz-checker/src/error_reporter/properties.rs`

## Verification

- `cargo check --package tsz-checker` passed.
- `cargo nextest run -p tsz-checker --lib ts2322_array_literal_elaboration_widens_destructuring_default_sources ts2322_array_literal_elaboration_preserves_same_primitive_literal_targets ts2345_array_literal_tuple_overflow_elaborates_element_mismatch_to_ts2322` passed (3/3).
- `./scripts/conformance/conformance.sh run --filter "destructuringParameterDeclaration1ES5" --verbose` passed (2/2).
- `./scripts/conformance/conformance.sh run --max 200` passed (200/200).
- Original implementation branch full run: `scripts/safe-run.sh --limit 75% -- ./scripts/conformance/conformance.sh run` completed with `12453/12582` passed, `+2` net (`12451 -> 12453`), and three FAIL-to-PASS improvements:
  - `TypeScript/tests/cases/compiler/normalizedIntersectionTooComplex.ts`
  - `TypeScript/tests/cases/compiler/objectLiteralExcessProperties.ts`
  - `TypeScript/tests/cases/conformance/es6/destructuring/destructuringParameterDeclaration1ES5.ts`
- The full run had one timeout, `TypeScript/tests/cases/compiler/mappedTypeRecursiveInference.ts`; a focused verbose rerun of `mappedTypeRecursiveInference` passed (2/2, no timeout).
