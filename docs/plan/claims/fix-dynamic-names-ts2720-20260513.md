# fix(checker): suppress false TS2720 for dynamic-name class implements

- **Date**: 2026-05-13
- **Branch**: `fix-dynamic-names-ts2720-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

Restore the `dynamicNames` conformance case on current `main`, which currently
emits an extra TS2720 for a class implementing another class through computed
property names. The fix should preserve real class-implements-class diagnostics
while avoiding a false-positive suggestion when the apparent mismatch is driven
by computed member identity.

## Files Touched

- TBD

## Verification

- TBD
