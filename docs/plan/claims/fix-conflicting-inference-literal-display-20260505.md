# fix(checker): align conflicting inference literal display

- **Date**: 2026-05-05
- **Branch**: `fix/conflicting-inference-literal-display-20260505`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance fingerprint parity

## Intent

Fix the remaining fingerprint-only divergence in
`TypeScript/tests/cases/compiler/typeInferenceConflictingCandidates.ts`.
Prior merged work made `tsz` emit the expected `TS2345`; this follow-up targets
the documented literal display mismatch where the diagnostic source/target text
uses widened primitive types instead of the literal forms that `tsc` prints.

## Files Touched

- TBD after verbose fingerprint analysis.

## Verification

- `./scripts/conformance/conformance.sh run --filter "typeInferenceConflictingCandidates" --verbose`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- Focused `cargo nextest run` for the owning-crate regression.
