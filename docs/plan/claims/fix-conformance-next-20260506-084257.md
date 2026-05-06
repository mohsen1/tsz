# fix(checker): align variadic tuples2 fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-084257`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Fix the remaining fingerprint-only conformance mismatch in
`TypeScript/tests/cases/conformance/types/tuple/variadicTuples2.ts`. The
current drift keeps the expected `TS1257`, `TS1265`, `TS1266`, `TS2322`, and
`TS2345` code set, but differs in rest-element diagnostic positions, tuple
alias-vs-structural display for assignment errors, and tuple-level call
argument elaboration for variadic rest tuples with trailing fixed elements.

This continues the older `claude/exciting-keller-3GYxU` investigation by
targeting the remaining fingerprints rather than the duplicate assignment
elaboration slice already documented there.

## Files Touched

- `docs/plan/claims/fix-conformance-next-20260506-084257.md`

## Verification

- Pending.
