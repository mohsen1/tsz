# [WIP] fix(checker): widen destructuring parameter default diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-destructuring-param-default-display`
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-05 10:25:28 UTC

## Intent

Fix the random conformance pick `destructuringParameterDeclaration1ES5.ts`,
where `tsc` and `tsz` emit the same TS2322/TS2345/TS2393/TS7006/TS7010/TS7031
codes but disagree on TS2322 source-type display for destructuring parameter
defaults. `tsc` reports widened primitives (`string`, `number`) for the
argument array elements; `tsz` currently reports literal types (`"string"`,
`1`, `2`).

## Files Touched

- `docs/plan/claims/fix-checker-destructuring-param-default-display.md`
- Production and regression-test files TBD after root-cause diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "destructuringParameterDeclaration1ES5" --verbose`.
- Planned: focused checker regression for destructuring parameter default
  diagnostic display.
- Planned: relevant `cargo check`, `cargo nextest run`, and conformance smoke
  before marking ready.
