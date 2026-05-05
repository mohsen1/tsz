# [WIP] fix(checker): align noImplicitAny indexing fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/no-implicit-any-indexing-fingerprint`
- **PR**: TBD
- **Status**: claim
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

- TBD after root-cause analysis.

## Verification

- Baseline target command:
  `./scripts/conformance/conformance.sh run --filter "noImplicitAnyIndexing" --verbose`
- Planned: owning-crate Rust regression test.
- Planned: targeted conformance rerun for `noImplicitAnyIndexing`.
