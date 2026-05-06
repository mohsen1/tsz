# fix(checker): suppress extra TS2304 for JSDoc satisfies tag typedef lookup

- **Date**: 2026-05-06
- **Branch**: `fix/jsdoc-satisfies-tag14-diagnostics`
- **PR**: #3629
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-06 05:01:00 UTC

## Intent

Fix the current conformance false positive in
`TypeScript/tests/cases/conformance/jsdoc/checkJsdocSatisfiesTag14.ts`.
TypeScript reports the expected parse diagnostic, while `tsz` also reports an
extra `TS2304` for the JSDoc `@satisfies T1` type references.

## Files Touched

- `docs/plan/claims/fix-jsdoc-satisfies-tag14-ts2304.md`
- implementation files to be identified during root-cause investigation
- owning-crate Rust regression test

## Verification

- live conformance run saved at `/tmp/tsz-current-conformance-b7b6871374.txt`
- targeted conformance rerun for `checkJsdocSatisfiesTag14`
- targeted owning-crate `cargo nextest run` regression test
