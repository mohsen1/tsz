# fix(checker): preserve failed Object.assign fallback assignment diagnostics

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next`
- **PR**: #2748
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

This PR targets the fingerprint-only failure in
`TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/unionAndIntersectionInference1.ts`.
`tsc` reports both the inner `TS2769` for an invalid generic `Object.assign`
call and the outer `TS2322` when the failed call's fallback return type is
assigned to an explicitly typed variable. `tsz` previously emitted the
overload failure but collapsed the generic helper return to `any`, suppressing
the outer assignment diagnostic.

## Files Touched

- `crates/tsz-checker/src/checkers/call_checker/overload_resolution.rs`
- `crates/tsz-checker/tests/ts2322_tests.rs`
- `crates/tsz-solver/src/intern/intersection.rs`
- `crates/tsz-solver/tests/intern_tests.rs`

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo nextest run --package tsz-checker --lib` (3332 passed, 10 skipped)
- `cargo nextest run --package tsz-solver --lib` (5622 passed, 9 skipped)
- `cargo nextest run -p tsz-checker --test ts2322_tests -E 'test(generic_object_assign_helper_keeps_outer_ts2322) | test(generic_object_assign_initializer_keeps_outer_ts2322)'`
- `cargo nextest run -p tsz-solver -E 'test(test_mixed_intersection_preserves_callable_object_order)'`
- `cargo build --profile dist-fast --bin tsz`
- `./scripts/conformance/conformance.sh run --filter "unionAndIntersectionInference1" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (FINAL RESULTS: 12437/12582 passed)
