# fix(checker): realign thisInFunctionCall diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/this-in-function-call-regression-20260506-193529`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

Target `TypeScript/tests/cases/compiler/thisInFunctionCall.ts`.
The canonical picker reports a fingerprint-only TS2683 mismatch. This slice
will identify the current source/span/message difference for implicit `this`
diagnostics in function-call contexts and realign checker diagnostic rendering
without changing the diagnostic code set.

## Files Touched

- TBD after investigation.

## Verification

- Focused Rust regression in the owning checker path.
- `./scripts/conformance/conformance.sh run --filter "thisInFunctionCall" --verbose`
