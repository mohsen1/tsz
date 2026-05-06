# fix(checker): align infer conditional type fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/checker-infer-types-fingerprints`
- **PR**: #3586
- **Status**: claim
- **Workstream**: 1 (conformance)

## Intent

Claiming `TypeScript/tests/cases/conformance/types/conditional/inferTypes1.ts`.

Current `origin/main` reports the expected TS1338, TS2304, TS2322, and TS2344
codes, but the diagnostic fingerprints are missing two TS2344 entries for
conditional `infer` constraint violations.

## Verification

- Pending.
