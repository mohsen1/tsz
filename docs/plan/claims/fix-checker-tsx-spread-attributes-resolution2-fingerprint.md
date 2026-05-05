# fix(checker): align TSX spread attributes resolution fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-tsx-spread-attributes-resolution2-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only conformance failure in
`TypeScript/tests/cases/conformance/jsx/tsxSpreadAttributesResolution2.tsx`.
Both tsc and tsz emit `TS2322` and `TS2739`, but the diagnostic fingerprints
do not match. The planned scope is to diagnose the exact JSX spread attribute
display or anchoring mismatch and fix it through the existing JSX
assignability/display paths.

## Files Touched

- `docs/plan/claims/fix-checker-tsx-spread-attributes-resolution2-fingerprint.md`

## Verification

- Pending
