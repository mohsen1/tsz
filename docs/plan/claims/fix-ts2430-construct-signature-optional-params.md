# [WIP] fix(checker): align TS2430 construct signature optional-parameter display

- **Date**: 2026-05-05
- **Branch**: `fix-ts2430-construct-signature-optional-params`
- **PR**: #2761
- **Status**: abandoned
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigated the TS2430 fingerprint-only mismatch in
`subtypingWithGenericConstructSignaturesWithOptionalParameters.ts`.
The picked failure reports the same diagnostic code as `tsc` in the stale
snapshot, but a targeted verbose run on current `origin/main` passes. This
claim is abandoned because there is no current failure to fix for the picked
test.

## Files Touched

- `docs/plan/claims/fix-ts2430-construct-signature-optional-params.md`
- No production or test files changed.

## Verification

- `./scripts/conformance/conformance.sh run --filter "subtypingWithGenericConstructSignaturesWithOptionalParameters" --verbose` (1/1 passed; no fingerprint-only mismatch).
