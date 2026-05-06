# [WIP] fix(checker): align typeArgumentInference fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-quick-pick-20260506-next`
- **PR**: #3650
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/conformance/expressions/functionCalls/typeArgumentInference.ts`.
The stored snapshot reports a fingerprint-only mismatch with matching
`TS2322`, `TS2345`, and `TS2403` code sets. This PR will root-cause the
generic call inference or diagnostic rendering drift and land the fix in the
owning checker/solver path with a focused Rust regression test.

## Files Touched

- `docs/plan/claims/fix-conformance-quick-pick-20260506-next.md`
- `crates/tsz-checker/src/types/computation/call_result.rs`
- `crates/tsz-checker/tests/conformance_issues/types/membership_semantics.rs`

## Verification

- `cargo fmt --check`
- `CARGO_BUILD_JOBS=2 cargo nextest run -p tsz-checker test_generic_literal_argument_error_preserves_direct_inference_literal direct_generic_argument_mismatch_survives_context_sensitive_callback dependent_type_parameter_constraint_checks_second_argument_against_first_inference`
- `CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --filter "expressions/functionCalls/typeArgumentInference.ts" --verbose`
- `CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --filter "typeArgumentInference" --verbose` (14/15 pass; remaining failure is `typeArgumentInferenceWithConstraints.ts` Window/TS2403 fingerprint drift outside this claim)
