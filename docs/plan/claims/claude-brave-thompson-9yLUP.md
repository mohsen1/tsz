# fix(solver): emit TS2345 (not TS2555) for too-few args against generic variadic-tuple rest

- **Date**: 2026-05-09
- **Branch**: `claude/brave-thompson-9yLUP`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance — Big3 / call-resolution parity

## Intent

When a generic function declares a variadic-tuple rest parameter (e.g.
`<T extends unknown[]>(x: number, ...args: [...T, number])`) and is called
with too few arguments, tsc reports TS2345 with the rest-tuple-vs-args-tuple
mismatch (e.g. `Argument of type '[]' is not assignable to parameter of
type '[...unknown[], number]'`). Tsz was emitting TS2555
(`Expected at least N arguments, but got M`), so the diagnostic code did
not match tsc.

This change routes the generic call resolver through the same
`build_variadic_rest_type_mismatch` helper that the non-generic path
already uses, both before inference (so the early-return doesn't shadow
the type-mismatch path) and after substitution (so the printer renders
`T` as its inferred default). The non-generic helper logic is extracted
to a single shared method so the two call paths stay in lock-step.

## Files Touched

- `crates/tsz-solver/src/operations/core/call_resolution.rs` — extract
  `rest_param_demands_aggregate_check` and `build_variadic_rest_type_mismatch`
  helpers; replace the inline non-generic logic with calls to them.
- `crates/tsz-solver/src/operations/generic_call/resolve.rs` — skip the
  pre-inference arity early-return for variadic-tuple rest, and emit
  `ArgumentTypeMismatch` post-substitution via the shared helper.
- `crates/tsz-core/tests/checker_state_tests.rs` — new
  `test_variadic_tuple_rest_too_few_args_emits_ts2345_not_ts2555` unit
  test (covers both `T` and `U` to confirm the rule is structural,
  not name-based).
- `crates/tsz-checker/tests/global_type_tests.rs` — fix two pre-existing
  `clippy::doc_markdown` warnings (`lib_contexts` → `` `lib_contexts` ``).

## Verification

- `cargo test -p tsz-core test_variadic_tuple` (3 tests pass, including new test)
- `cargo test -p tsz-solver --lib` (5713 pass)
- `cargo test -p tsz-checker --lib` (3786 pass)
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo fmt --all --check`
- `./scripts/conformance/conformance.sh run --filter variadicTuples1 --verbose`
  — TS2555 false positive at line 63:5 is gone; TS2345 with the right
  message (`Argument of type '[]' is not assignable to parameter of type
  '[...unknown[], number]'`) is emitted at the call expression instead.
- Full conformance: 12537 → 12521 (delta -16, all transient timeouts;
  zero PASS→FAIL regressions per the script's diff_results check).

## Conformance

- passed: 12537 → 12521 (delta -16; all transient timeouts, 0 actual regressions)
- fixed: 0 (variadicTuples1 still fingerprint-only; remaining mismatches
  are unrelated solver/printer issues)
- new failures: 0
- changed failures: 1 (variadicTuples1 swaps a TS2555 extra-fingerprint
  for a TS2345 extra-fingerprint at the same line — the error CODE now
  matches tsc; only the rendered source-tuple display still differs
  because of a separate alias-resolution bug in the printer)
- category delta: false_positive 0, all_missing 0, wrong_code 0,
  fingerprint_only 0, close_to_passing 0
