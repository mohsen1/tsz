# fix(parser): align plain JS binder error diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/parser-plain-js-binder-errors-diagnostics`
- **PR**: #3675
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the remaining missing diagnostic codes in
`TypeScript/tests/cases/conformance/salsa/plainJSBinderErrors.ts`. The prior
TS1101 slice covered strict-mode `with`; this follow-up targets the remaining
plain-JS binder/parser diagnostics reported by the quick-pick:
`TS1102`, `TS1107`, `TS1210`, `TS1214`, `TS1215`, `TS1359`, and `TS18012`.

## Files Touched

- `crates/tsz-checker/src/context/strict_mode.rs`
- `crates/tsz-checker/src/state/state_checking/strict_names.rs`
- `crates/tsz-checker/src/state/state_checking_members/statement_checks.rs`
- `crates/tsz-checker/src/state/variable_checking/core.rs`
- `crates/tsz-checker/src/types/computation/helpers.rs`
- `crates/tsz-checker/tests/conformance_issues/errors/error_cases.rs`
- `crates/tsz-cli/src/driver/check_utils.rs`
- `crates/tsz-parser/src/parser/state_statements_class_members.rs`
- `crates/tsz-parser/tests/parser_improvement_tests.rs`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-parser --lib test_plain_js_strict_binder_parse_diagnostics_are_preserved`
- `cargo nextest run -p tsz-cli --lib filtered_parse_diagnostics_keeps_await_ts1359_with_unrelated_parse_errors js_parse_allowlist_keeps_plain_js_binder_strict_codes`
- `cargo nextest run -p tsz-checker --test conformance_issues test_plain_js_binder_errors_use_module_and_cross_function_diagnostics`
- `./scripts/conformance/conformance.sh run --filter "plainJSBinderErrors" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
