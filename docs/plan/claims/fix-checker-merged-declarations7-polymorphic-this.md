# [WIP] fix(checker): preserve export equals polymorphic this diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-merged-declarations7-fingerprint`
- **PR**: #3064
- **Status**: ready
- **Workstream**: 1 (Conformance)

## Intent

Fix the fingerprint-only conformance failure in
`TypeScript/tests/cases/compiler/mergedDeclarations7.ts`. The test expects
TS2322 to report `PassportStatic` assigned to `Passport` when a method returning
polymorphic `this` is called through an `export =` namespace import.

## Files Touched

- `crates/tsz-checker/src/assignability/assignability_diagnostics.rs`
- `crates/tsz-checker/src/assignability/polymorphic_this_diagnostics.rs`
- `crates/tsz-checker/src/assignability/mod.rs`
- `crates/tsz-checker/src/state/type_resolution/module.rs`
- `crates/tsz-checker/tests/conformance_issues/features/elaboration.rs`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker architecture_contract_tests_src::test_no_inline_solver_function_calls_in_checker_modules`
- `cargo nextest run -p tsz-checker --test conformance_issues test_export_equals_named_import_preserves_polymorphic_this_ts2322 test_overloaded_interface_method_inheritance_uses_trailing_signature_compatibility`
- `cargo nextest run -p tsz-checker --test conformance_issues` => `851 passed, 12 skipped`
- `./scripts/conformance/conformance.sh run --filter "mergedDeclarations7" --verbose` => `1/1 passed`
- `./scripts/conformance/conformance.sh run --max 200` => `200/200 passed`
