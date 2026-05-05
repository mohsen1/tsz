# fix(checker): suppress variance annotation constraint cascade

- **Date**: 2026-05-05
- **Branch**: `fix/checker-variance-annotations-extra-ts2344`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the only-extra conformance failure in
`TypeScript/tests/cases/conformance/types/typeParameters/typeParameterLists/varianceAnnotations.ts`.
tsc reports the expected variance annotation diagnostics without an additional
generic constraint failure, while tsz currently emits an extra `TS2344`.

## Files Touched

- `docs/plan/claims/fix-checker-variance-annotations-extra-ts2344.md`

## Verification

- Pending
