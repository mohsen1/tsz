# fix(checker): align unused locals/parameters fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/checker-unused-locals-parameters-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only conformance failure in
`TypeScript/tests/cases/compiler/unusedLocalsAndParameters.ts`.
Both tsc and tsz emit `TS1005` and `TS1109`, but diagnostic fingerprints do
not match. The planned scope is to identify the exact parser/checker diagnostic
location or message drift and fix it through the existing diagnostic paths.

## Files Touched

- `docs/plan/claims/fix-checker-unused-locals-parameters-fingerprint.md`

## Verification

- Pending
