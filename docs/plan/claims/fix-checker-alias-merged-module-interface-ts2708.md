# [WIP] fix(checker): report TS2708 for aliased merged module interfaces

- **Date**: 2026-04-29
- **Branch**: `fix/checker-alias-merged-module-interface-ts2708`
- **PR**: #1708
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the `aliasOnMergedModuleInterface.ts` conformance failure selected by
`scripts/session/quick-pick.sh`, where TSZ missed TS2708 for use of a
plain `import = require(...)` alias whose `export =` target resolves to an
uninstantiated merged namespace/interface symbol. The fix keeps this in the
checker name-resolution boundary and leaves type-position access like `foo.A`
valid while reporting value-position access like `foo.bar(...)`.

## Files Touched

- `crates/tsz-checker/src/types/queries/type_only.rs`
- `crates/tsz-checker/tests/name_resolution_boundary_tests.rs`

## Verification

- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run -p tsz-checker ts2708_plain_import_equals_require_to_uninstantiated_export_equals_namespace`
- `cargo nextest run -p tsz-cli compile_alias_on_merged_module_interface_fixture_reports_ts2708`
- `cargo nextest run --package tsz-checker --lib` (2961 passed, 11 skipped)
- `./scripts/conformance/conformance.sh run --filter "aliasOnMergedModuleInterface" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed; targeted fixture improved)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (`FINAL RESULTS: 12236/12582 passed (97.3%)`)
