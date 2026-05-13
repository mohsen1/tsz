# fix(checker): bind call-signature this returns (#6383)

- **Date**: 2026-05-13
- **Branch**: `fix-this-call-signature-return-6383-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / public false-positive fixes

## Intent

Fix #6383, where a callable interface with a call signature returning `this` produces a TS2741 false positive when the call result is assigned back to the same interface. The fix should preserve TypeScript's receiver identity for `this` return types in call signatures without weakening general assignability.

## Files Touched

- TBD

## Verification

- TBD
