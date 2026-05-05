# [WIP] fix(checker): align control-flow iteration error fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-control-flow-iteration-error-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-05

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/conformance/controlFlow/controlFlowIterationErrors.ts`.
`tsz` already reports the expected `TS2345` and `TS2769` codes, but the
diagnostic fingerprints differ from `tsc`. This PR will root cause the
message/span mismatch, add focused Rust regression coverage in the owning
crate, and verify the targeted conformance case.

## Files Touched

- `docs/plan/claims/fix-checker-control-flow-iteration-error-fingerprints.md`
- Production and regression-test files TBD after root-cause diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "controlFlowIterationErrors" --verbose`.
- Planned: focused Rust regression test in the owning crate.
- Planned: `cargo check`, focused `cargo nextest run`, and conformance smoke before marking ready.
