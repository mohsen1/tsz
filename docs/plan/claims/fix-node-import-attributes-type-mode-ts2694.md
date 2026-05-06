# fix(checker): suppress TS2694 for node import attributes type mode

- **Date**: 2026-05-06
- **Branch**: `fix/node-import-attributes-type-mode-ts2694`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)
- **Claimed**: 2026-05-06 02:17:15 UTC

## Intent

Fix the current conformance false positive in
`TypeScript/tests/cases/conformance/node/nodeModulesImportAttributesTypeModeDeclarationEmit.ts`.
The live conformance run reports that TypeScript emits no diagnostics, while
`tsz` emits an extra `TS2694`.

## Files Touched

- `docs/plan/claims/fix-node-import-attributes-type-mode-ts2694.md`
- implementation files to be identified during root-cause investigation
- owning-crate Rust regression test

## Verification

- live conformance run saved at `/tmp/tsz-current-conformance-claim2.txt`
- targeted conformance rerun for `nodeModulesImportAttributesTypeModeDeclarationEmit`
- targeted owning-crate `cargo nextest run` regression test
