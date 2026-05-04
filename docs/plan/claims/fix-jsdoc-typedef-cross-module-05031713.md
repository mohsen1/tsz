# [WIP] fix(checker): report cross-module JSDoc typedef collisions

- **Date**: 2026-05-03
- **Branch**: `fix/jsdoc-typedef-cross-module-05031713`
- **PR**: #2587
- **Status**: implemented
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigate and fix `jsdoc/typedefCrossModule5.ts`, where `tsc` reports
TS2300 and TS2451 for a checked-JS cross-file collision between a JSDoc
`@typedef`, a class, and a const, while `tsz` currently emits no diagnostics.

## Files Touched

- `crates/tsz-checker/src/jsdoc/diagnostics.rs`
- `crates/tsz-checker/src/types/type_checking/duplicate_identifiers.rs`
- `crates/tsz-checker/tests/js_jsdoc_diagnostics_tests.rs`

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/conformance/jsdoc/typedefCrossModule5.ts`
  with expected `[TS2300, TS2451]` and actual `[]`.
- `cargo nextest run -p tsz-checker checked_js_cross_file_typedef_and_script_globals_duplicate`
- `cargo nextest run -p tsz-checker jsdoc_cross_file_typedef_tests`
- `cargo nextest run -p tsz-checker cross_file_class_vs_remote_const_uses_ts2451 cross_file_let_vs_remote_const_uses_ts2451`
- `cargo nextest run -p tsz-checker checked_js_constructor_var_merges_with_class_without_false_duplicates_or_new_errors`
- `cargo nextest run -p tsz-checker jsdoc_cross_file_typedef_tests checked_js_cross_file_typedef_and_script_globals_duplicate checked_js_constructor_var_merges_with_class_without_false_duplicates_or_new_errors cross_file_class_vs_remote_const_uses_ts2451 cross_file_let_vs_remote_const_uses_ts2451`
- `cargo check -p tsz-checker`
- `cargo build --profile dist-fast --bin tsz >/tmp/tsz-build.log 2>&1 && ./scripts/conformance/conformance.sh run --filter "typedefCrossModule5" --verbose`
- `./scripts/conformance/conformance.sh run --filter "typedefCrossModule5" --verbose`
