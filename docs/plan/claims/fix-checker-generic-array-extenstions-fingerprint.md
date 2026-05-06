# fix(checker): align generic array extensions fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/checker-generic-array-extenstions-fingerprint`
- **PR**: Pending
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only conformance failure in
`TypeScript/tests/cases/compiler/genericArrayExtenstions.ts`.
Both tsc and tsz emit `TS2420`, but diagnostic fingerprints do not match. The
planned scope is to identify the exact checker diagnostic location or message
drift and fix it through the existing diagnostic path.

## Files Touched

- `docs/plan/claims/fix-checker-generic-array-extenstions-fingerprint.md`

## Verification

- Pending
