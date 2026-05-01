# Fix overloaded method with overloaded arguments

- **Date**: 2026-05-01
- **Branch**: `codex/fix-overloaded-method-overloaded-args`
- **PR**: #1954
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the remaining `genericCallToOverloadedMethodWithOverloadedArguments.ts`
conformance miss. Current `main` is missing the TS2345/TS2769 overload
failure pair for an overloaded generic method call where the argument itself is
overloaded, so the work likely spans overload resolution, generic call
inference, and diagnostic finalization.

## Files Touched

- `crates/tsz-solver/src/operations/` (expected)
- `crates/tsz-checker/src/` or checker integration tests (expected)

## Verification

- `cargo fmt --check`
- `cargo test -p tsz-checker --test generic_call_inference_tests overloaded_function_argument_uses_last_signature_for_generic_callback_inference -- --nocapture`
- `cargo test -p tsz-checker --test generic_call_inference_tests`
- `cargo build -p tsz-cli -p tsz-conformance`
- `.target/debug/tsz-conformance --test-dir TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/debug/tsz --filter genericCallToOverloadedMethodWithOverloadedArguments --verbose --print-fingerprints --workers 1`
