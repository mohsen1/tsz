# [WIP] fix(checker): align ambient const enum module diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429`
- **PR**: #1781
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the quick-picked conformance failure `verbatimModuleSyntaxAmbientConstEnum.ts`.
TSZ currently emits TS2748 but misses TS2300 and TS2432 for the ambient const enum
module scenario. This PR will diagnose the root cause in the parser/binder/checker
boundary and add a focused regression test in the owning crate.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/duplicate_identifiers.rs`
- `crates/tsz-checker/src/types/type_checking/duplicate_identifiers_helpers.rs`
- `crates/tsz-checker/tests/conformance_issues/core/fixtures.rs`

## Verification

- `cargo fmt --check`
- `git diff --check`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo nextest run -p tsz-checker test_declare_global_const_enum_reports_rebound_member_diagnostics`
- `./scripts/conformance/conformance.sh run --filter "verbatimModuleSyntaxAmbientConstEnum" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `cargo nextest run --package tsz-checker --lib` observed two unrelated pre-existing failures: `architecture_contract_tests_src::test_solver_imports_go_through_query_boundaries` reports `declarations/import/core/import_members_tests.rs` importing `tsz_solver::TypeInterner`, and `ts2322_tests::test_ts2322_assignment_destructuring_defaults_report_undefined_mismatches` reports 4 TS2322 diagnostics where the test expects 3.
