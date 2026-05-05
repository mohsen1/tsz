# [WIP] fix(conformance): align generic function inference diagnostics

- **Date**: 2026-05-05
- **Branch**: `conformance/generic-function-inference1-20260505`
- **PR**: #3047
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the picked `genericFunctionInference1.ts` conformance mismatch. The current
fingerprint expects TS2345 but tsz also emits TS2322 and TS2362, so the work
will identify whether the extra diagnostics come from generic inference,
contextual typing, or arithmetic operand checking after failed inference.

## Files Touched

- `crates/tsz-checker/src/checkers/call_checker/overload_resolution.rs`
- `crates/tsz-checker/src/checkers/call_context.rs`
- `crates/tsz-checker/src/error_reporter/call_errors_tests.rs`
- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/src/types/computation/call/mod.rs`
- `crates/tsz-checker/src/types/computation/call_display.rs`

## Verification

- `cargo fmt --all`
- `cargo check`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-checker --lib pipe_contextual_return_refines_overload_callbacks_progressively nested_generic_call_callee_receives_outer_call_context`
- `cargo nextest run --package tsz-checker --lib` (3426 passed, 10 skipped)
- `./scripts/conformance/conformance.sh run --filter "genericFunctionInference1" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (`FINAL RESULTS: 12455/12582 passed (99.0%)`)
