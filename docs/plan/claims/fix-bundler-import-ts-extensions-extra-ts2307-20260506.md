# fix(resolver): suppress bundler TS extension fallback TS2307

- **Date**: 2026-05-06
- **Branch**: `fix/bundler-import-ts-extensions-extra-ts2307-20260506-151625`
- **PR**: #4124
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Target `TypeScript/tests/cases/conformance/moduleResolution/bundler/bundlerImportTsExtensions.ts`.
tsz currently emits one extra TS2307 while tsc only reports the bundler/extension
diagnostics TS2846, TS5024, TS5097, and TS6142. This PR will identify the
module-resolution or import-diagnostic path that lets an unresolved extension
candidate fall through to TS2307.

## Files Touched

- `crates/tsz-checker/src/declarations/import/declaration.rs`
- `crates/tsz-cli/src/driver/tests.rs`
- `crates/tsz-cli/tests/driver_tests.rs`

## Verification

- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-cli -E 'test(compile_bundler_dts_value_import_reports_ts2846_not_ts2307)'`
- `cargo fmt --check`
- `./scripts/conformance/conformance.sh run --filter "bundlerImportTsExtensions" --verbose`
