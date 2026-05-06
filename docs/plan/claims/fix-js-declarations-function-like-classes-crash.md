# fix(checker): stop crash in JS declarations function-like classes

- **Date**: 2026-05-06
- **Branch**: `fix/js-declarations-function-like-classes-crash`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-06 06:52:00 UTC

## Intent

Fix the current conformance crash in
`TypeScript/tests/cases/conformance/jsdoc/declarations/jsDeclarationsFunctionLikeClasses2.ts`.
The targeted current-main conformance run reports the test as `CRASH`.

## Files Touched

- `docs/plan/claims/fix-js-declarations-function-like-classes-crash.md`
- implementation files to be identified during root-cause investigation
- owning-crate Rust regression test

## Verification

- targeted conformance rerun for `jsDeclarationsFunctionLikeClasses2`
- targeted owning-crate `cargo nextest run` regression test
