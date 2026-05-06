# fix(checker): align combined rest parameter diagnostics

- **Date**: 2026-05-06
- **Branch**: `codex/signature-rest-combining-20260506`
- **PR**: #3633
- **Status**: implemented
- **Workstream**: 1 (Conformance)

## Intent

Fix the `TypeScript/tests/cases/compiler/signatureCombiningRestParameters5.ts`
conformance mismatch. The current filtered run emits `TS2345` for the first
array argument with a literal array display (`true[]`) and misses the second
combined-signature rest parameter diagnostic.

The expected impact is a one-test conformance pass-rate increase without
changing unrelated overload or rest-parameter diagnostics.

## Files Touched

- `docs/plan/claims/codex-signature-rest-combining-20260506.md`
- `crates/tsz-checker/src/checkers/call_checker/diagnostics.rs`
- `crates/tsz-checker/src/checkers/call_checker/overload_resolution.rs`
- `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`
- `crates/tsz-checker/src/query_boundaries/common.rs`
- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/src/types/computation/call_inference.rs`
- `crates/tsz-checker/src/types/computation/call_result.rs`
- `crates/tsz-checker/tests/union_index_access_function_application_param_tests.rs`

## Verification

- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test union_index_access_function_application_param_tests signature_combining_rest_parameters_5_reports_both_rest_argument_mismatches -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test union_index_access_function_application_param_tests signature_combining_rest_parameters_4_preserves_intersection_display_order -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test union_index_access_function_application_param_tests -- --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --lib architecture_contract_tests_src::test_checker_sources_forbid_solver_internal_imports_typekey_usage_and_raw_interning -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --lib architecture_contract_tests_src::test_solver_imports_go_through_query_boundaries -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --lib contextual_typing_tests::test_no_false_ts2345_for_mapped_tuple_rest_spread -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --lib contextual_typing_tests::test_parenthesized_conditional_callbacks_preserve_contextual_typing -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --lib contextual_typing_tests::test_contextual_function_object_property_intersection_sequence -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --lib error_reporter::call_errors::tests::ts2345_generic_call_parameter_display_preserves_instantiated_alias_name -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test contextual_typing_tests test_no_false_ts2345_for_mapped_tuple_rest_spread -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test contextual_typing_tests test_parenthesized_conditional_callbacks_preserve_contextual_typing -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test contextual_typing_tests test_contextual_function_object_property_intersection_sequence -- --exact --nocapture`
- `CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test generic_call_inference_tests contextual_nested_generator_return_inference_drops_stale_ts2345 -- --exact --nocapture`
- `cargo fmt --check`
- `git diff --check`
- `rg -n "eprintln!|dbg!|println!" ...` (no matches)

Conformance was attempted with the pinned TypeScript fixture, but the
`cargo build --profile dist-fast -p tsz-cli -p tsz-conformance` step failed
with `No space left on device` before the conformance binary could be run.
