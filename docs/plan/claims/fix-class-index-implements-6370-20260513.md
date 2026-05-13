# fix(checker): suppress duplicate TS2420 for class index implements (#6370)

- **Date**: 2026-05-13
- **Branch**: `fix-class-index-implements-6370-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / public false-positive fixes

## Intent

Fix #6370, where a class implementing an index-signature-only interface emits a duplicate TS2420 even though the class declares a compatible index signature. Preserve the expected member/index TS2411 diagnostic for incompatible named members.

## Files Touched

- TBD

## Verification

- TBD
