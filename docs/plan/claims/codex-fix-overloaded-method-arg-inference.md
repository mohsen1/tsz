# Fix overloaded method argument inference diagnostics

- **Date**: 2026-05-01
- **Branch**: `codex/fix-overloaded-method-arg-inference`
- **PR**: #1923
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix a hard remaining conformance failure in overload/call inference, starting with
`genericCallToOverloadedMethodWithOverloadedArguments.ts`, which currently misses
both TS2345 and TS2769. The likely scope spans checker call diagnostics,
contextual typing/inference, and solver relation applicability rather than a
single diagnostic-printer tweak.

## Files Touched

- `crates/tsz-solver/src/operations/constraints/signatures.rs`
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`

## Verification

- `cargo test -p tsz-checker --test generic_call_inference_tests` (75 tests pass)
- `.target/debug/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/debug/tsz --filter genericCallToOverloadedMethodWithOverloadedArguments --verbose --print-fingerprints --workers 1` (1/1 passed)
