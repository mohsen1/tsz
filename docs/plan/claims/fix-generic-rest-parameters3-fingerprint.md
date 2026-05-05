# fix(checker): align generic rest parameter diagnostics

- **Date**: 2026-05-04
- **Branch**: `fix/generic-rest-parameters3-fingerprint`
- **PR**: #2732
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the fingerprint-only diagnostic divergence in
`genericRestParameters3.ts`. The expected and actual diagnostic codes already
matched (`TS2322`, `TS2345`, `TS2554`); this PR aligns the remaining
rest-argument diagnostics, generic aggregate-rest display, and tuple-list rest
signature assignability.

## Files Touched

- `crates/tsz-checker/src/checkers/call_checker/candidate_collection.rs`
- `crates/tsz-checker/src/query_boundaries/checkers/call.rs`
- `crates/tsz-checker/src/types/computation/call_finalize.rs`
- `crates/tsz-checker/src/types/computation/call_result.rs`
- `crates/tsz-checker/tests/spread_rest_tests.rs`
- `crates/tsz-checker/tests/conformance_issues/features/function_shape.rs`
- `crates/tsz-solver/src/diagnostics/format/mod.rs`
- `crates/tsz-solver/src/diagnostics/format/tests.rs`
- `crates/tsz-solver/src/operations/call_args.rs`
- `crates/tsz-solver/src/operations/generic_call/normalization.rs`
- `crates/tsz-solver/src/operations/generic_call/resolve.rs`
- `crates/tsz-solver/src/relations/subtype/rules/functions/checking.rs`
- `crates/tsz-solver/src/type_queries/data/content_predicates.rs`

## Verification

- `cargo nextest run -p tsz-checker --test spread_rest_tests test_array_like_rest_rejects_aggregate_rest_arguments test_tuple_union_rest_rejects_aggregate_rest_arguments`
- `cargo nextest run -p tsz-checker --test conformance_issues test_tuple_union_rest_target_requires_all_variants_for_fixed_source test_tuple_union_rest_with_equivalent_merged_prefix_assigns_both_directions`
- `cargo nextest run -p tsz-checker --test spread_rest_tests test_tuple_union_rest_accepts_matching_tuple_spreads test_tuple_union_rest_rejects_aggregate_rest_arguments`
- `cargo nextest run -p tsz-checker --test spread_rest_tests` (`75/75 passed`)
- `cargo nextest run -p tsz-solver diagnostics::format::tests::format_function_type_param_with_non_primitive_array_constraint_uses_generic_form diagnostics::format::tests::format_function_type_param_with_structural_array_constraint_uses_shorthand diagnostics::format::tests::format_function_type_param_with_array_application_constraint_preserves_generic_form`
- `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance && ./.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary ./.target/dist-fast/tsz --filter genericRestParameters3 --workers 1 --verbose --print-fingerprints` (`1/1 passed`, no fingerprint-only failures)
- Regression repair after CI aggregate caught four regressions:
  - `classImplementsMethodWIthTupleArgs` (`1/1 passed`)
  - `genericRestParameters1` (`1/1 passed`)
  - `assignmentCompatWithCallSignatures3` (`1/1 passed`)
  - `objectSpreadStrictNull` (`1/1 passed`)
- Pre-commit hook on latest commit: clippy zero warnings, wasm rustc warning gate, architecture guardrails, `20670` tests passed, `65` skipped.
