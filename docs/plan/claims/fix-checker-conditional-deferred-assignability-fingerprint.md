# fix(checker): align deferred conditional assignability fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-conditional-deferred-assignability-fingerprint`
- **PR**: #2774
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/compiler/conditionalTypeAssignabilityWhenDeferred.ts`.
The picked test already emits the same diagnostic codes as `tsc`
(`TS2322`, `TS2345`), so this PR will root-cause the remaining message,
display, count, or anchor mismatch around deferred conditional type
assignability.

## Files Touched

- `docs/plan/claims/fix-checker-conditional-deferred-assignability-fingerprint.md`
  (claim)
- `crates/tsz-solver/src/evaluation/evaluate_rules/conditional.rs`
- `crates/tsz-solver/src/relations/subtype/cache.rs`
- `crates/tsz-solver/src/relations/subtype/rules/conditionals.rs`
- `crates/tsz-solver/src/diagnostics/format/compound.rs`
- `crates/tsz-checker/src/assignability/assignability_checker.rs`
- `crates/tsz-checker/src/types/computation/call_inference.rs`
- `crates/tsz-checker/src/types/computation/call_result.rs`
- `crates/tsz-checker/src/assignability/assignment_checker_tests.rs`

## Verification

- `cargo nextest run --package tsz-solver --lib`
  - not run: `cargo-nextest` is not installed in this environment
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo test -p tsz-checker deferred_conditional_target_assignability_fingerprints --lib -- --nocapture`
- `cargo test -p tsz-checker generic_call_with_this_indexed_conditional_parameter_reports_ts2345 --lib -- --nocapture`
- `cargo test -p tsz-checker ts2589_tests::bounded_recursive_conditional_no_ts2589_at_definition --lib -- --nocapture`
- `cargo test -p tsz-core checker_state_tests::test_redux_pattern_generic_function_with_conditional_return --lib -- --nocapture`
- `cargo test --package tsz-checker --lib`
  - `test result: ok. 3343 passed; 0 failed; 10 ignored`
- `cargo test --package tsz-solver --lib`
  - `test result: ok. 5626 passed; 0 failed; 9 ignored`
- `./scripts/conformance/conformance.sh run --filter "conditionalTypeAssignabilityWhenDeferred" --verbose`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
  - `Fingerprint-only: 0`
- `./scripts/conformance/conformance.sh run --max 200`
  - `FINAL RESULTS: 200/200 passed (100.0%)`
  - `Fingerprint-only: 0`
