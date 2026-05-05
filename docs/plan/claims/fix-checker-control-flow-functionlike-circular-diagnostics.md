# [WIP] fix(checker): control-flow function-like circular diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-control-flow-functionlike-circular-diagnostics`
- **PR**: #2931
- **Status**: ready
- **Workstream**: conformance / missing diagnostics

## Intent

Random conformance pick selected
`TypeScript/tests/cases/compiler/controlFlowFunctionLikeCircular1.ts`.
The test is currently only-missing: `tsz` emits the existing TDZ/circular
subset but misses several `tsc` diagnostics in the multi-file case.

Baseline on `origin/main` (`a1793fcb3d0`):

- Expected codes: `TS1155`, `TS2345`, `TS2355`, `TS2393`, `TS2411`,
  `TS2448`, `TS2451`, `TS2454`, `TS2456`, `TS2502`, `TS2554`, `TS2749`
- Actual codes: `TS1155`, `TS2355`, `TS2448`, `TS2451`, `TS2454`,
  `TS2502`, `TS2749`
- Missing codes: `TS2345`, `TS2393`, `TS2411`, `TS2456`, `TS2554`

Key missing fingerprints:

- `TS2345` on the two `unionOfDifferentReturnType1(true)` calls
- `TS2393` duplicate function implementation diagnostics for repeated
  `function test(...)` declarations across the virtual files
- `TS2411` for property `x` conflicting with a string index signature
- `TS2456` for `type First = typeof arg`
- `TS2554` for calling the predicate-like value with zero arguments

## Files Touched

- `crates/tsz-checker/src/types/type_checking/duplicate_identifiers.rs`
- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/src/types/computation/call_helpers.rs`
- `crates/tsz-solver/src/operations/core/call_resolution.rs`
- `crates/tsz-checker/src/state/state_checking_members/index_signature_checks.rs`
- `crates/tsz-checker/src/state/type_analysis/computed_helpers.rs`
- `crates/tsz-checker/src/state/type_analysis/core_type_query.rs`
- `crates/tsz-checker/src/types/type_node.rs`
- `crates/tsz-checker/src/types/type_node_advanced.rs`
- `crates/tsz-checker/src/types/type_node_helpers.rs`
- Focused regression tests under `crates/tsz-checker/tests/` and
  `crates/tsz-checker/src/error_reporter/call_errors_tests.rs`

## Verification

- Baseline captured with
  `./scripts/conformance/conformance.sh run --filter "controlFlowFunctionLikeCircular1" --verbose`
- `cargo test -p tsz-checker --lib test_type_literal_union_function_property_vs_index_signature -- --nocapture`
- `cargo test -p tsz-checker --test type_alias_typeof_circular_tests test_ts2456_typeof_parameter_after_forward_predicate_call -- --nocapture`
- `cargo test -p tsz-checker --test ts2300_tests duplicate_script_function_implementations_across_files_emit_ts2393 -- --nocapture`
- `cargo test -p tsz-checker --lib tdz_callee -- --nocapture`
- `cargo test -p tsz-checker --test call_resolution_regression_tests union_callee_intersects_any_with_specific_parameter_type -- --nocapture`
- `cargo fmt --all --check`
- `./scripts/conformance/conformance.sh run --filter "controlFlowFunctionLikeCircular1" --verbose` passes 1/1 with no fingerprint-only mismatch.
