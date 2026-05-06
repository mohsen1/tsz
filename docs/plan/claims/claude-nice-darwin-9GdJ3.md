# fix(checker): widen mutable bindings copied from unannotated const literals

- **Date**: 2026-05-06
- **Branch**: `claude/nice-darwin-9GdJ3`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance / checker — mutable binding widening

## Intent

Closes #3446. A mutable binding initialized from an unannotated `const`
string/number/bigint/boolean literal (`const tag = "start"; let m = tag;`)
must widen to the primitive (`string`) the same way a direct literal
initializer widens. tsc treats unannotated const literals as widening
literal types; tsz currently only widens when the initializer syntax is
itself a fresh literal token, so the literal type leaks into the
`let`/`var` binding and rejects later legal assignments.

The structural rule:

> When a mutable binding's initializer is an identifier resolving to an
> unannotated `const` declaration whose initializer is itself a fresh
> literal expression, that identifier is also a fresh literal expression
> for widening purposes.

## Files Touched

- `crates/tsz-checker/src/types/utilities/core.rs` (extend
  `is_fresh_literal_expression` with the const-identifier case + cycle
  guard).
- `crates/tsz-checker/src/state/variable_checking/...` (regression test).

## Verification

- 7 new unit tests in
  `crates/tsz-checker/src/tests/mutable_binding_widening_from_const_literal_tests.rs`
  covering string/number/var widening, transitive const-chains, the
  explicitly-typed const negative case, the `let`-source negative
  case, and a non-looping `const a = a` self-reference smoke test.
- `cargo test -p tsz-checker --lib` (3583 passed, 0 failed).
- Manual repro from issue body now matches tsc:
  `index.ts(6,7): error TS2322: Type 'string' is not assignable to type '"start"'.`
