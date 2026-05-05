# [WIP] fix(checker): align noImplicitAny indexing fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/no-implicit-any-indexing-fingerprint`
- **PR**: #3189
- **Status**: abandoned
- **Workstream**: 1 (Conformance / diagnostic fingerprint)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/compiler/noImplicitAnyIndexing.ts`, a fingerprint-only
failure where tsc and tsz emit the same diagnostic code set
(`TS2339`, `TS7015`, `TS7053`) but disagree on one or more diagnostic
fingerprints. This PR will identify the shared indexing diagnostic invariant,
fix the owning checker/solver boundary path, and add Rust regression coverage
before rerunning the targeted conformance test.

## Files Touched

- `docs/plan/claims/fix-no-implicit-any-indexing-fingerprint.md`

## Verification

- Abandoned before production changes because the claim was created from a
  stale snapshot. Current `origin/main` already has
  `TypeScript/tests/cases/compiler/noImplicitAnyIndexing.ts` as PASS in
  `scripts/conformance/conformance-baseline.txt`, and the test is absent from
  `scripts/conformance/conformance-detail.json`.
- No checker/solver code was changed.
