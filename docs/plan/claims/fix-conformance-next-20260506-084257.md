# fix(checker): align variadic tuples2 fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-084257`
- **PR**: #3888
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Fix the remaining fingerprint-only conformance mismatch in
`TypeScript/tests/cases/conformance/types/tuple/variadicTuples2.ts`. The
current drift keeps the expected `TS1257`, `TS1265`, `TS1266`, `TS2322`, and
`TS2345` code set, but differs in rest-element diagnostic positions, tuple
alias-vs-structural display for assignment errors, and tuple-level call
argument elaboration for variadic rest tuples with trailing fixed elements.

This continues the older `claude/exciting-keller-3GYxU` investigation by
targeting the remaining fingerprints rather than the duplicate assignment
elaboration slice already documented there.

## Files Touched

- `docs/plan/claims/fix-conformance-next-20260506-084257.md`
- `crates/tsz-checker/src/checkers/call_checker/candidate_collection.rs`
- `crates/tsz-checker/src/context/core.rs`
- `crates/tsz-checker/src/error_reporter/call_errors/display_formatting.rs`
- `crates/tsz-checker/src/error_reporter/call_errors/display_formatting_variadic.rs`
- `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source/assignment_formatting.rs`
- `crates/tsz-checker/src/error_reporter/core_formatting.rs`
- `crates/tsz-checker/src/types/computation/call_result.rs`
- `crates/tsz-checker/src/types/type_node.rs`
- `crates/tsz-checker/src/types/type_node_helpers.rs`
- `crates/tsz-checker/tests/spread_rest_tests.rs`
- `crates/tsz-checker/tests/variadic_tuple_elaboration_tests.rs`
- `crates/tsz-solver/src/operations/call_args.rs`
- `crates/tsz-solver/tests/operations_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=.target cargo check -p tsz-checker -p tsz-solver`
- `CARGO_TARGET_DIR=.target cargo nextest run -p tsz-checker test_ts1265_rest_after_rest_array_type_references direct_variadic_tuple_annotation_uses_structural_target_display variadic_rest_tuple_call_trailing_mismatch_uses_tuple_level_error generic_spread_rest_tuple_with_trailing_callback_uses_aggregate_display constrained_readonly_variadic_tuple_call_uses_constraint_surface`
- `CARGO_TARGET_DIR=.target cargo nextest run -p tsz-solver test_call_variadic_tuple_rest_with_trailing_element_uses_aggregate_mismatch`
- `./scripts/conformance/conformance.sh run --filter "variadicTuples2"`
- `./scripts/conformance/conformance.sh run --max 200`
