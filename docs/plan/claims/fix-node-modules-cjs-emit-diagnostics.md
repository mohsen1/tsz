# fix(checker): recover node CJS emit diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/node-modules-cjs-emit-diagnostics`
- **PR**: #3126
- **Status**: ready
- **Workstream**: conformance / node modules diagnostics

## Intent

Random conformance pick selected
`TypeScript/tests/cases/conformance/node/nodeModulesCJSEmit1.ts`.
tsc reports `TS1192`, `TS2304`, and `TS2882`, while tsz currently reports
only `TS2882`. This PR will root-cause why the CJS/node emit scenario drops
the default-export and missing-name diagnostics, then add the owning-crate
regression coverage.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/computed_commonjs/exports_detection.rs`
- `crates/tsz-checker/src/assignability/assignment_checker/commonjs_assignment.rs`
- `crates/tsz-checker/src/types/computation/identifier/resolution.rs`
- `crates/tsz-checker/src/types/property_access_helpers/expando.rs`
- `crates/tsz-checker/src/types/property_access_type/helpers.rs`
- `crates/tsz-checker/src/types/property_access_type/resolve.rs`
- `crates/tsz-checker/src/query_boundaries/js_exports.rs`
- `crates/tsz-checker/src/state/type_resolution/module.rs`
- `crates/tsz-checker/src/state/type_resolution/module/interop.rs`
- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `crates/tsz-checker/src/declarations/import/core/import_members.rs`
- `crates/tsz-checker/src/declarations/import/core/mod.rs`
- `crates/tsz-checker/src/declarations/import/core/type_only_js.rs`
- `crates/tsz-cli/src/driver/check.rs`
- `crates/tsz-cli/src/driver/check_utils.rs`
- `crates/tsz-core/src/config/mod.rs`

## Verification

- `cargo test -p tsz-cli test_collect_diagnostics_rejects_exports_in_cjs_file_with_esm_syntax -- --nocapture`
- `cargo test -p tsz-checker --test conformance_issues test_esm_declaration_module_without_default_still_reports_ts1192 -- --nocapture`
- `cargo test -p tsz-checker test_checker_file_size_ceiling -- --nocapture`
- `cargo fmt --check`
- `cargo nextest run -p tsz-checker -p tsz-solver`
- `./scripts/conformance/conformance.sh run --filter "nodeModulesCJSEmit1" --verbose`
