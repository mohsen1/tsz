# [WIP] fix(checker): report CommonJS export property redeclarations

- **Date**: 2026-05-05
- **Branch**: `fix/checker-commonjs-export-property-redeclare-diagnostic`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

This PR aligns the checker with TypeScript's TS2323 diagnostic for CommonJS modules that assign overlapping properties before and after `module.exports = A`. The selected conformance case is `moduleExportWithExportPropertyAssignment4`, where tsz currently reports the downstream TS2339 but misses the exported variable redeclaration.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/computed_commonjs/exports_detection.rs`
- `crates/tsz-checker/tests/js_export_surface_tests.rs`

## Verification

- Pending
