# [WIP] fix(checker): report destructuring tuple diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-destructuring-tuple-diagnostics`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-05

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/compiler/destructuringTuple.ts`.
`tsz` currently reports no diagnostics where `tsc` reports `TS2488` and
`TS2769`. This PR will root cause the checker divergence, add focused Rust
regression coverage in the owning crate, and verify the targeted conformance
case.

## Files Touched

- `docs/plan/claims/fix-checker-destructuring-tuple-diagnostics.md`
- Production and regression-test files TBD after root-cause diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "destructuringTuple" --verbose`.
- Planned: focused Rust regression test in the owning checker crate.
- Planned: `cargo check`, focused `cargo nextest run`, and conformance smoke before marking ready.
