# fix(checker): align thisless contextual inference diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-thisless-functions-contextual-inference`
- **PR**: #2759
- **Status**: ready
- **Workstream**: conformance / contextual inference and diagnostic fingerprints

## Intent

Random conformance pick selected `TypeScript/tests/cases/compiler/thislessFunctionsNotContextSensitive1.ts`.
The compact picker reports an extra `TS18046`; the verbose run shows the full
test still fails on a combination of contextual-inference false positives and
display fingerprints. This PR will root-cause the shared inference/display
rules needed to make the test match `tsc`, without adding checker-local
single-test suppressions.

Observed verbose mismatch on `origin/main`:

- Extra `TS18046` at `state123.bar2` in a Vuex-style mutation callback.
- `TS2345` fingerprints for `NonStringIterable<T>` calls render
  `NonStringIterable<unknown>` and over-report array arguments where `tsc`
  only rejects the string literal with target `never`.
- `TS2820` target display expands `ExtractFields<...>` into a literal union
  where `tsc` preserves the conditional/mapped alias surface.

## Files Touched

- `crates/tsz-solver/src/operations/constraints/walker.rs` (conditional target inference)
- `crates/tsz-checker/src/types/computation/call_helpers.rs` (object-literal inference from callable union members)
- `crates/tsz-checker/src/error_reporter/*` (TS2820 target display preservation)
- `crates/tsz-solver/tests/operations_tests.rs` (conditional inference regression)
- `crates/tsz-checker/tests/generic_call_inference_tests.rs` (checker inference regression)
- `crates/tsz-checker/tests/conformance_issues/features/import_aliases.rs` (Vuex-style callback regression)

## Verification

- `cargo fmt --all --check`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo nextest run -p tsz-solver test_infer_generic_conditional_param_with_check_placeholder_from_branch` (1 passed)
- `cargo nextest run -p tsz-checker conditional_parameter_infers_through_branches_before_assignability test_no_false_ts18046_union_state_function_infers_top_level_state` (2 passed)
- `cargo nextest run -p tsz-checker mapped_type_recursive_inference_generic_call_preserves_nested_callback_context` (2 passed)
- `./scripts/conformance/conformance.sh run --filter "thislessFunctionsNotContextSensitive1" --verbose` (1/1 passed, fingerprint-only 0)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed, fingerprint-only 0)
