# fix(checker): align generic class function member fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/generic-class-function-member-fingerprint`
- **PR**: #3128
- **Status**: verified
- **Workstream**: conformance / contextual signature instantiation

## Intent

Random conformance pick selected
`TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/genericClassWithFunctionTypedMemberArguments.ts`.
The divergence was fingerprint-only: both tsc and tsz report `TS2345`, but tsz
rewrote the finalized generic expected type parameter to a sibling literal in
the call-site diagnostic. TSC preserves the annotated callback parameter's
outer type parameter in the TS2345 parameter slot.

This claim takes over the stale investigation in
`docs/plan/claims/claude-brave-thompson-cj2vT.md`; there is no open PR for
that branch. The implementation fixes the call-result diagnostic path so
finalized expected types that still contain type parameters are emitted without
a second assignability recheck that can literalize the target. Owning-crate
regression coverage was added for the selected conformance case.

## Files Touched

- `crates/tsz-checker/src/types/computation/call_result.rs`
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`

## Verification

- Baseline target command failed with fingerprint-only mismatches:
  `./scripts/conformance/conformance.sh run --filter "genericClassWithFunctionTypedMemberArguments" --verbose`
- Passed:
  `cargo nextest run -p tsz-checker --test generic_call_inference_tests generic_class_function_member_annotated_callback_keeps_outer_type_param_in_ts2345 direct_generic_argument_mismatch_survives_context_sensitive_callback`
- Passed:
  `cargo nextest run -p tsz-checker --test generic_call_inference_tests`
- Passed:
  `./scripts/conformance/conformance.sh run --filter "genericClassWithFunctionTypedMemberArguments" --verbose`
