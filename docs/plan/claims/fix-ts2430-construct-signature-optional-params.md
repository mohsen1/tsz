# [WIP] fix(checker): align TS2430 construct signature optional-parameter display

- **Date**: 2026-05-05
- **Branch**: `fix-ts2430-construct-signature-optional-params`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigate and fix the TS2430 fingerprint-only mismatch in
`subtypingWithGenericConstructSignaturesWithOptionalParameters.ts`.
The picked failure reports the same diagnostic code as `tsc`, so this slice
is scoped to the exact message/fingerprint divergence around generic construct
signatures with optional parameters.

## Files Touched

- `docs/plan/claims/fix-ts2430-construct-signature-optional-params.md`
- Production and test files TBD after root-cause diagnosis.

## Verification

- Planned: targeted verbose conformance run for `subtypingWithGenericConstructSignaturesWithOptionalParameters`.
- Planned: owning-crate unit tests for the fixed invariant.
- Planned: relevant `cargo check`, `cargo nextest run`, and conformance regression checks before marking ready.
