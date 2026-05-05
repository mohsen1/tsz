# fix(parser): align fuzz array recovery fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/parser-fuzz-array-recovery-fingerprint`
- **PR**: #3289
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only conformance failure in
`TypeScript/tests/cases/conformance/parser/ecmascript5/Fuzz/parser0_004152.ts`.
TypeScript recovers from the malformed array expression in a class property
initializer by reporting repeated `TS1005` semicolon expectations, while tsz
currently recovers as if the remaining comma-separated expressions were class
members and reports repeated `TS1068` diagnostics.

## Files Touched

- `docs/plan/claims/fix-parser-fuzz-array-recovery-fingerprint.md`

## Verification

- Pending
