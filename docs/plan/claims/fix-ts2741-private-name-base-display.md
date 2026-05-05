# [WIP] fix(checker): display private-name missing property source base

- **Date**: 2026-05-05
- **Branch**: `fix-ts2741-private-name-base-display`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the refreshed random conformance pick `privateNamesUnique-4.ts`, where
`tsc` and `tsz` both emit TS2564 and TS2741 but disagree on the TS2741
fingerprint. For assignment `const c: C = a` where `A2 extends A1`, tsc reports
private `#something` missing in source type `A1`; tsz currently reports `A2`.

## Files Touched

- `docs/plan/claims/fix-ts2741-private-name-base-display.md`
- Production and regression-test files TBD after root-cause diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "privateNamesUnique-4" --verbose`.
- Planned: focused checker regression for TS2741 private-name source display.
- Planned: relevant `cargo check`, `cargo nextest run`, and conformance smoke before marking ready.
