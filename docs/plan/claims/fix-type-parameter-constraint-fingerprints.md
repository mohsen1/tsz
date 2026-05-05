# fix(checker): align type parameter constraint diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/type-parameter-constraint-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

This PR targets the fingerprint-only conformance mismatch in
`TypeScript/tests/cases/conformance/types/typeParameters/typeArgumentLists/typeParameterAsTypeParameterConstraint2.ts`.
The current code set already matches TypeScript (`TS2322`, `TS2345`, and
`TS2454`), so the fix will inspect diagnostic anchors, messages, and displayed
types for the remaining fingerprint divergence.

## Files Touched

- TBD

## Verification

- Pending
