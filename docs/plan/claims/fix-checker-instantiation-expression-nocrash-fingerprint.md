# [WIP] fix(checker): align instantiation expression no-crash fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-instantiation-expression-nocrash-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance / diagnostic fingerprints)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/compiler/instantiationExpressionErrorNoCrash.ts`,
a fingerprint-only failure where tsz and tsc agree on diagnostic codes
`TS2344` and `TS2635` but differ in diagnostic fingerprint details.

This PR will root-cause the remaining instantiation-expression no-crash
fingerprint mismatch, add owning Rust regression coverage, and rerun the
targeted conformance test.

## Files Touched

- TBD after root-cause analysis.

## Verification

- Baseline target command:
  `./scripts/conformance/conformance.sh run --filter "instantiationExpressionErrorNoCrash" --verbose`
- Planned: owning-crate Rust regression test.
- Planned: targeted conformance rerun for `instantiationExpressionErrorNoCrash`.
