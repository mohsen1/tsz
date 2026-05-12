# fix(checker): report merged Baz variance assignment

- **Date**: 2026-05-12
- **Branch**: `fix/variance-annotations-baz-20260512`
- **Base**: `fix/variance-annotations-anon-class-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

Continue the `varianceAnnotations.ts` conformance cleanup by targeting the remaining missing diagnostic on the merged `Baz` interface assignment:

- `baz1 = baz2;` should report `TS2322 test.ts:117:1 Type 'Baz<string>' is not assignable to type 'Baz<unknown>'.`

This is expected to require semantic handling of conflicting variance annotations across merged interface declarations, not another display-only cleanup.

## Files Touched

- `docs/plan/claims/fix-variance-annotations-baz-20260512.md`

## Verification

- Pending focused baseline on stacked branch.
