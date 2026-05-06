# fix(checker): align union type reduction2 arity display

- **Date**: 2026-05-06
- **Branch**: `fix/checker-union-type-reduction2-arity-display`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

This PR targets the quick-picked fingerprint-only conformance mismatch in
`TypeScript/tests/cases/conformance/types/union/unionTypeReduction2.ts`.
The diagnostic code set already matches TypeScript (`TS2554`), but the
fingerprint text or anchor differs.

## Context

Selected with `scripts/session/quick-pick.sh --seed 3519`. The normal
`--run` path could not initialize `TypeScript` because this checkout has
`.gitmodules` metadata but no tracked `TypeScript` gitlink, so the pick used
the existing `scripts/conformance/conformance-detail.json` after the failed
submodule attempt.

## Files Touched

- TBD

## Verification

- Pending
