# [WIP] fix(checker): report elided import duplicate diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/checker-elided-js-import-duplicates`
- **PR**: #1716
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the `elidedJSImport1.ts` conformance failure selected by the session picker.
`tsz` currently reports `TS2591` and `TS18042` but misses `TS2300` and `TS2708`
for the duplicate elided-JS import shape. The implementation will identify the
root cause in the checker/binder boundary and add an owning Rust regression test.

## Files Touched

- `docs/plan/claims/fix-checker-elided-js-import-duplicates.md`
- `crates/tsz-checker/src/declarations/import/core/import_members.rs`
- `crates/tsz-checker/src/declarations/import/equals.rs`
- `crates/tsz-checker/src/state/state_checking_members/statement_callback_bridge.rs`
- `crates/tsz-checker/src/types/computation/identifier/core.rs`
- `crates/tsz-checker/tests/js_jsdoc_diagnostics_tests.rs`

## Verification

- `cargo check --package tsz-checker` (pass)
- `cargo check --package tsz-solver` (pass)
- `cargo build --profile dist-fast --bin tsz` (pass)
- `cargo nextest run -p tsz-checker --test js_jsdoc_diagnostics_tests` (9 tests pass)
- `./scripts/conformance/conformance.sh run --test-dir "$tmpdir" --filter "elidedJSImport1" --verbose` (1/1 pass; temporary test dir used because this disposable worktree's TypeScript submodule checkout could not fetch the pinned SHA)
- `cargo nextest run --package tsz-checker --lib` (blocked by pre-existing unrelated failures: `architecture_contract_tests_src::checker_files_stay_under_loc_limit`, `enum_nominality_tests::test_number_literal_to_numeric_enum_type`)
