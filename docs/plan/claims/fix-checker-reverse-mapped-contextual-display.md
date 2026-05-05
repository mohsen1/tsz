# fix(checker): preserve reverse mapped contextual diagnostic display

- **Date**: 2026-05-06
- **Branch**: `fix/checker-reverse-mapped-contextual-display`
- **PR**: https://github.com/mohsen1/tsz/pull/3464
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

This PR targets the quick-picked fingerprint-only conformance mismatch in
`TypeScript/tests/cases/compiler/reverseMappedTypeContextualTypeNotCircular.ts`.
The diagnostic code set already matches TypeScript (`TS2322`), but the rendered
type fingerprint differs.

## Context

`docs/plan/claims/investigate-diagnostic-type-display-alias-preservation.md`
lists this fixture as part of a broader alias/application display hand-off. This
claim narrows the work to the reverse-mapped contextual display case selected by
`scripts/session/quick-pick.sh --seed 3407`.

## Files Touched

- TBD

## Verification

- Pending
