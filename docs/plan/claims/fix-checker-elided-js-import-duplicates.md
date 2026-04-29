# [WIP] fix(checker): report elided import duplicate diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/checker-elided-js-import-duplicates`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the `elidedJSImport1.ts` conformance failure selected by the session picker.
`tsz` currently reports `TS2591` and `TS18042` but misses `TS2300` and `TS2708`
for the duplicate elided-JS import shape. The implementation will identify the
root cause in the checker/binder boundary and add an owning Rust regression test.

## Files Touched

- `docs/plan/claims/fix-checker-elided-js-import-duplicates.md`
- Implementation files TBD after diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "elidedJSImport1" --verbose`
- Planned: targeted crate `cargo nextest run` for touched crates.
