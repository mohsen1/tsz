# [WIP] fix(checker): align mapped type as-clause TS2345 fingerprint

- **Date**: 2026-04-29
- **Branch**: `fix/mapped-type-as-clauses-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

This PR targets the quick-picked conformance failure
`TypeScript/tests/cases/conformance/types/mapped/mappedTypeAsClauses.ts`, a
fingerprint-only `TS2345` mismatch. The work will diagnose why tsz reports the
same code with a different fingerprint from `tsc`, fix the root cause in the
appropriate checker/solver/formatting boundary, and add a focused Rust
regression test for the invariant.

## Files Touched

- TBD after diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "mappedTypeAsClauses" --verbose`
- Planned: targeted `cargo nextest run` for touched crate(s)
