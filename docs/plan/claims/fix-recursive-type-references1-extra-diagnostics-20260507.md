# fix(checker): reduce recursive type references extra diagnostics

- **Date**: 2026-05-07
- **Branch**: `fix/recursive-type-references1-extra-diagnostics-20260507-000000`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

Target
`TypeScript/tests/cases/conformance/types/typeRelationships/recursiveTypes/recursiveTypeReferences1.ts`.
The canonical picker reports extra diagnostics: expected `TS2304,TS2322`,
actual `TS2304,TS2322,TS2339,TS7006,TS7031`. This slice will identify why
recursive type-reference recovery leaks extra property and implicit-any
diagnostics, then fix the owning checker path without suppressing the expected
missing-name and assignability diagnostics.

## Files Touched

- TBD after investigation.

## Verification

- Focused Rust regression in the owning path.
- `./scripts/conformance/conformance.sh run --filter "recursiveTypeReferences1" --verbose`
