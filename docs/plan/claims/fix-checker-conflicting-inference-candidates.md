# fix(checker): reject conflicting inference candidates

- **Date**: 2026-04-28
- **Branch**: `fix/checker-conflicting-inference-candidates`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the conformance gap in `typeInferenceConflictingCandidates.ts`, where a generic call with incompatible inference candidates currently succeeds instead of reporting TS2345. The fix should preserve TypeScript's best-common-candidate inference behavior while rejecting genuinely conflicting candidates in the shared call resolution path.

## Files Touched

- `crates/tsz-solver/src/operations/generic_call/inference_helpers.rs`
- `crates/tsz-checker/src/types/computation/call_inference.rs`
- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`

## Verification

- `cargo check -p tsz-checker -p tsz-solver`
- `cargo nextest run -p tsz-checker -E 'test(direct_generic_argument_mismatch_survives_context_sensitive_callback) or test(direct_generic_argument_mismatch_is_not_recovered_to_success)'`
- `cargo fmt --check --package tsz-checker --package tsz-solver`
- Direct conformance runner for `typeInferenceConflictingCandidates`: TS2345 now emitted; remaining mismatch is fingerprint-only literal display (`number`/`string` vs `3`/`""`).
