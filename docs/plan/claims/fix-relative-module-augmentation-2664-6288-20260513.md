# fix(checker): report TS2664 for unresolved relative module augmentations

- **Date**: 2026-05-13
- **Branch**: `fix-relative-module-augmentation-2664-6288-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance / module resolution)

## Intent

Issue #6288 reports that `declare module "./nonexistent"` is accepted even though tsc emits TS2664 for unresolved relative module augmentations. The fix should stay in module-augmentation validation / resolution plumbing and add a focused regression for the relative path case, preserving existing package-name TS2664 behavior.

## Files Touched

- TBD

## Verification

- TBD
