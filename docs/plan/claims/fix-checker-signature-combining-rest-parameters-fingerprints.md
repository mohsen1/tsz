# fix(checker): align signature combining rest parameter fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/checker-signature-combining-rest-parameters-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (conformance)

## Intent

Claiming `TypeScript/tests/cases/compiler/signatureCombiningRestParameters5.ts`.

Current `origin/main` reports the expected TS2345 code, but the diagnostic
fingerprints differ for rest-parameter signature combination. The first error
prints `true[]` where TSC prints `boolean[]`, and the second callback error is
missing the expected `number[]` argument diagnostic.

## Verification

- Pending.
