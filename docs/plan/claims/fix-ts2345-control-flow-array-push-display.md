# [WIP] fix(checker): align control-flow array push TS2345 fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix-ts2345-control-flow-array-push-display`
- **PR**: #2799
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the refreshed random conformance pick `controlFlowArrayErrors.ts`, where
`tsc` and `tsz` emit the same TS2345/TS7005/TS7034 codes but disagree on two
TS2345 fingerprints for evolving-array `push` calls. The expected TS2345
surfaces are `99` against `never` and `"hello"` against `number`.

## Files Touched

- `docs/plan/claims/fix-ts2345-control-flow-array-push-display.md`
- Production and regression-test files TBD after root-cause diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "controlFlowArrayErrors" --verbose`.
- Planned: focused checker regression for the array push diagnostic surface.
- Planned: relevant `cargo check`, `cargo nextest run`, and conformance smoke before marking ready.
