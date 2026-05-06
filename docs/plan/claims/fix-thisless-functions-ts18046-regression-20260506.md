# fix(checker): remove thisless contextual callback TS18046 regression

- **Date**: 2026-05-06
- **Branch**: `fix/thisless-functions-ts18046-regression-20260506-165500`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

Target `TypeScript/tests/cases/compiler/thislessFunctionsNotContextSensitive1.ts`.
The current conformance picker reports one extra TS18046 even though the target
was previously fixed by PR #2759. This PR will re-run the current fixture,
identify the remaining or regressed contextual-callback inference path, and fix
the root cause without a target-specific suppression.

## Files Touched

- TBD after investigation.

## Verification

- Focused Rust regression in the owning checker/solver area.
- `./scripts/conformance/conformance.sh run --filter "thislessFunctionsNotContextSensitive1" --verbose`.
