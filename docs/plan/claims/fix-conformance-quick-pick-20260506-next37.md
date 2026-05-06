# [WIP] fix(checker): align prop-types validator inference

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-quick-pick-20260506-next37`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Claiming `TypeScript/tests/cases/compiler/propTypeValidatorInference.ts`.

Current `origin/main`, once the stable-Rust emitter build blocker from #3932 is applied, emits one extra `TS2322` for the final `ExtractPropsMatch` assertion where tsc accepts the inferred `PropTypes.InferProps` shape.

This slice will correct the checker/solver inference behavior without suppressing unrelated assignment diagnostics.

## Files Touched

- TBD after root-cause investigation.

## Verification

- Baseline: `./scripts/conformance/conformance.sh run --filter "propTypeValidatorInference" --verbose` reaches 0/1 with one extra TS2322 after applying the #3932 build unblock.
