# fix(solver): avoid false TS2349 for compatible union this signatures

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429-3`
- **PR**: #1804
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigate and fix the quick-pick replacement fingerprint failure in
`unionTypeCallSignatures6.ts`. The original quick-pick target
`contextualParamTypeVsNestedReturnTypeInference4.ts` already passes on current
`origin/main`, so this PR tracks the live union-call-signature mismatch instead:
`x1.f2()` emitted a false TS2349 even though the receiver satisfies the
intersection of the single signature's `this` type and one overload's `this`
type.

## Files Touched

- `docs/plan/claims/fix-conformance-quick-pick-20260429-3.md`
- `crates/tsz-solver/src/operations/core/call_evaluator.rs`
- `crates/tsz-solver/src/operations/core/call_resolution.rs`
- `crates/tsz-solver/tests/operations_tests.rs`
- `crates/tsz-checker/tests/call_resolution_regression_tests.rs`
- `crates/tsz-checker/src/tests/union_multi_overload_unified_sig_tests.rs`

## Verification

- `./scripts/conformance/conformance.sh run --filter "contextualParamTypeVsNestedReturnTypeInference4" --verbose` (already passes on current `origin/main`; stale picker entry)
- `cargo check --package tsz-solver`
- `cargo check --package tsz-checker`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-solver --lib test_union_call_mixed_overloads_intersects_this_types_callable test_union_call_multi_overloads_structurally_identical_this_callable test_union_call_mixed_overloads_compatible_this_callable`
- `cargo nextest run --package tsz-checker --test call_resolution_regression_tests union_single_and_multi_overload_matching_this_no_ts2349 union_single_and_multi_overload_intersected_this_no_ts2349 union_multi_overload_incompatible_this_emits_ts2349 union_multi_overload_compatible_this_no_ts2349`
- `cargo nextest run --package tsz-checker --lib union_single_plus_multi_overload_no_match_emits_ts2349 union_single_plus_multi_overload_rejects_via_unified_sig union_single_plus_multi_overload_accepts_matching_arg`
- `./scripts/conformance/conformance.sh run --filter "unionTypeCallSignatures6" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `cargo nextest run --package tsz-solver --lib` (5551 passed, 9 skipped)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (`12244/12582 passed`, 97.3%)
