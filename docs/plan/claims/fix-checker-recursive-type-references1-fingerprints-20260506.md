# [WIP] fix(checker): align recursiveTypeReferences1 fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/recursive-type-references1-fingerprints-20260506`
- **PR**: #3778
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the quick-picked fingerprint-only target
`TypeScript/tests/cases/conformance/types/typeRelationships/recursiveTypes/recursiveTypeReferences1.ts`.
The diagnostic code set matches TypeScript (`TS2304`, `TS2322`), but tsz
reports different TS2322 positions and messages around recursive aliases and
recursive array inference. This slice is scoped to root-causing that checker
drift and landing focused Rust coverage in the owning path.

## Files Touched

- `docs/plan/claims/fix-checker-recursive-type-references1-fingerprints-20260506.md`
  (claim)

## Verification

- Pending
