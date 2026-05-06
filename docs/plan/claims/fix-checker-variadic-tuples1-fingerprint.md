# fix(checker): align variadic tuples1 fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/checker-variadic-tuples1-fingerprint`
- **PR**: #3530
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only conformance failure in
`TypeScript/tests/cases/conformance/types/tuple/variadicTuples1.ts`.
Both tsc and tsz emit `TS2322`, `TS2344`, `TS2345`, `TS2555`, and `TS4104`,
but diagnostic fingerprints do not match. The planned scope is to identify the
exact tuple diagnostic location or rendering drift and fix it through the
existing checker/solver diagnostic path.

## Files Touched

- `docs/plan/claims/fix-checker-variadic-tuples1-fingerprint.md`

## Verification

- Pending
