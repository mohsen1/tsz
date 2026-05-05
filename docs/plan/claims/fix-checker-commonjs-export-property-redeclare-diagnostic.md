# [WIP] fix(checker): report CommonJS export property redeclarations

- **Date**: 2026-05-05
- **Branch**: `fix/checker-commonjs-export-property-redeclare-diagnostic`
- **PR**: #3258
- **Status**: ready
- **Workstream**: conformance

## Intent

This PR aligns the checker with TypeScript's TS2323 diagnostic for CommonJS modules that assign overlapping properties before and after `module.exports = A`. The selected conformance case is `moduleExportWithExportPropertyAssignment4`, where tsz currently reports the downstream TS2339 but misses the exported variable redeclaration.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/computed_commonjs/exports_detection.rs`
- `crates/tsz-cli/tests/driver_tests.rs`

## Verification

- `cargo fmt --check`
- `cargo clippy -p tsz-checker -p tsz-cli --all-targets -- -D warnings`
- `CARGO_INCREMENTAL=0 cargo nextest run -p tsz-cli compile_commonjs_export_property_overlap_with_ambient_module_reports_ts2323 test_collect_diagnostics_keeps_ts1362_for_checked_js_module_exports_type_only_require` (new regression passed; pre-existing TS1362 test failed)
- `./scripts/conformance/conformance.sh run --filter "moduleExportWithExportPropertyAssignment4" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
