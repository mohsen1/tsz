# fix(TS7030): suppress false positive when return type includes undefined

- **Date**: 2026-05-12
- **Branch**: `claude/busy-knuth-BiDpr`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

Two bugs caused TS7030 ("Not all code paths return a value") to fire incorrectly:

1. The CLI `--strict` expansion incorrectly set `noImplicitReturns = true`. TypeScript's `--strict` does not include `noImplicitReturns`.
2. `should_skip_no_implicit_return_check` did not suppress TS7030 when the annotated return type was a union containing `undefined` (e.g. `string | undefined`). An implicit fall-through returning `undefined` is type-safe when `undefined` is part of the declared return type.

Generator functions are handled differently: their `TReturn` is checked separately and the suppression does not apply in the union-member path.

## Files Touched

- `crates/tsz-cli/src/driver/core.rs` (remove `no_implicit_returns` from `--strict` expansion)
- `crates/tsz-checker/src/checkers/promise_checker.rs` (`should_skip_no_implicit_return_check` logic)
- `crates/tsz-checker/src/state/state_checking_members/ambient_signature_checks.rs` (call sites updated)
- `crates/tsz-checker/src/state/state_checking_members/function_declaration_checks.rs` (call site updated)
- `crates/tsz-checker/src/types/function_type_helpers.rs` (call site updated)
- `crates/tsz-checker/tests/ts7030_undefined_union_return_tests.rs` (new unit tests)
- `crates/tsz-checker/src/lib.rs` (test module registration)
- `crates/tsz-cli/src/driver/tests.rs` (new CLI test)

## Verification

- `cargo test -p tsz-checker --lib ts7030` — 4 tests pass
- `cargo test -p tsz-cli --lib test_cli_strict_does_not_enable_no_implicit_returns` — passes
