# [WIP] fix(checker): align unknown control-flow fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-unknown-control-flow-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the remaining fingerprint-only conformance drift in
`TypeScript/tests/cases/conformance/types/unknown/unknownControlFlow.ts`.
Previous merged slices handled unknown-like union assignability, explicit
unknown-intersection TS2367 emission, and keyof display; this claim is scoped
to the current picker result on `origin/main`, where the diagnostic code set
already matches `tsc` (`TS2322`, `TS2345`, `TS2367`, `TS2536`) but one or more
fingerprints still differ.

## Files Touched

- TBD

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- targeted Rust regression test in the owning crate
- `./scripts/conformance/conformance.sh run --filter "unknownControlFlow" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
