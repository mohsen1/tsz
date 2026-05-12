# fix(checker): align recursiveTypeReferences1 TS2322 fingerprints

- **Date**: 2026-05-12
- **Branch**: `fix/recursive-type-references1-display-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

Continue the `recursiveTypeReferences1.ts` conformance work after prior slices removed extra diagnostic codes. This slice will inspect the current fingerprint-only TS2322 drift and fix the smallest display or anchor mismatch that can be owned by checker diagnostics without broad recursive-type semantic changes.

## Files Touched

- `docs/plan/claims/fix-recursive-type-references1-display-20260512.md`

## Verification

- Pending baseline: focused `recursiveTypeReferences1` conformance run on current main.
