# fix(checker): TS2355 missing for never-call variable initializers (#3662)

- **Date**: 2026-05-08
- **Branch**: `claude/nice-darwin-KH8hS`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / TS2355 parity

## Intent

`tsz` suppresses `TS2355` when a function with a non-void declared return
type ends with `const value = fail()` where `fail()` returns `never`.
TypeScript treats only **expression-statement-level** never calls as
terminating control flow; variable declarations whose initializers are
never calls still fall through. Align `tsz` with that rule by removing
the `VARIABLE_STATEMENT` special case in `statement_falls_through`.

Closes #3662.

## Files Touched

- `crates/tsz-checker/src/flow/reachability_checker.rs` (drop ~30 LOC
  of `VARIABLE_STATEMENT` special-casing)
- `crates/tsz-checker/tests/never_initializer_falls_through_tests.rs`
  (new unit test, locks in the structural rule)

## Verification

- `cargo nextest run -p tsz-checker --lib`
- `cargo nextest run -p tsz-checker --test never_initializer_falls_through_tests`
- conformance delta inspected after `scripts/conformance/conformance.sh snapshot`
