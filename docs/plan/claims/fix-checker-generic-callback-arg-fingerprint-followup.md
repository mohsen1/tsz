# [WIP] fix(checker): align generic callback TS2345 fingerprint

- **Date**: 2026-04-29
- **Branch**: `fix/checker-generic-callback-arg-fingerprint-followup`
- **PR**: #1778
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only TS2345 mismatch in
`genericCallbackInvokedInsideItsContainingFunction1.ts`. The targeted
conformance run reports matching `TS2345`/`TS2558` codes, but tsz currently
misses the expected `Argument of type 'U' is not assignable to parameter of
type 'T'.` fingerprint for the `f(y)` call.

## Files Touched

- `crates/tsz-checker/src/types/computation/call_result.rs` (~8 LOC)
- `crates/tsz-checker/tests/generic_tests.rs` (+22 LOC)
- `crates/tsz-solver/tests/integration_tests.rs` (+24 LOC)

## Verification

- `cargo fmt --all --check`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-checker --lib` (2969 passed, 11 skipped)
- `cargo nextest run --package tsz-solver --lib` (5546 passed, 9 skipped)
- `./scripts/conformance/conformance.sh run --filter "genericCallbackInvokedInsideItsContainingFunction1" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (`FINAL RESULTS: 12241/12582 passed (97.3%)`)
