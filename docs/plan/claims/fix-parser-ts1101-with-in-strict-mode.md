# fix(parser): emit TS1101 for `with` statements in strict-mode context

- **Date**: 2026-05-01
- **Branch**: `fix/parser-ts1101-with-in-strict-mode`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — missing parser diagnostics)

## Intent

`'with' statements are not allowed in strict mode.` (TS1101) is a
parser-level diagnostic that tsc emits when the `with` keyword is
encountered inside an auto-strict context — class bodies (always
strict per ECMA spec) and modules (auto-strict per spec). tsz had the
diagnostic data wired but no emit site, so the test
`compiler/conformance/salsa/plainJSBinderErrors.ts` (and any other
code that exercises `with` inside a class method) was missing the
diagnostic.

## Files Touched

- `crates/tsz-parser/src/parser/state_declarations_exports.rs`
  (`parse_with_statement`: emit TS1101 at the `with` keyword span when
  `in_strict_mode_context()` is true).
- `crates/tsz-checker/src/tests/ts1101_with_in_strict_mode_tests.rs`
  (3 locking unit tests: positive in class body, anti-hardcoding
  renamed cover, module-top-level cover).
- `crates/tsz-checker/src/lib.rs` (test module wiring).

## Verification

- `cargo nextest run -p tsz-parser -p tsz-checker -p tsz-solver --lib`
  → all green (8669 + 734 lib tests pass).
- Smoke conformance: `--filter with` → 20/20 PASS.
- `compiler/conformance/salsa/plainJSBinderErrors.ts` parent test still
  fails because 7 other missing-diagnostic codes (TS1102, TS1107,
  TS1210, TS1214, TS1215, TS1359, TS18012) are still missing — those
  are scoped to follow-up PRs.
