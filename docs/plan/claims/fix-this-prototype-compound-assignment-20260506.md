# fix(checker): suppress checked js prototype compound false positive

- **Date**: 2026-05-06
- **Branch**: `fix/this-prototype-compound-assignment-20260506-234200`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

Target
`TypeScript/tests/cases/conformance/jsdoc/thisPrototypeMethodCompoundAssignmentJs.ts`.
The canonical picker reports a false-positive diagnostic: expected no
diagnostics, actual `TS2531`. This slice will identify why checked JavaScript
prototype-method compound assignment treats the receiver as nullable, and fix
the owning checker path without suppressing unrelated nullability diagnostics.

## Files Touched

- TBD after investigation.

## Verification

- Focused Rust regression in the owning path.
- `./scripts/conformance/conformance.sh run --filter "thisPrototypeMethodCompoundAssignmentJs" --verbose`
