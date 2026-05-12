# fix: infer from function rest param satisfies unknown[] constraint (issue #5796)

- **Date**: 2026-05-12
- **Branch**: `claude/stoic-cori-27vUG`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance / TS2344 false positive

## Intent

When `infer A` appears in a rest-parameter position of a function type in a
conditional extends clause (e.g. `T extends (...args: infer A) => infer R`),
TypeScript implicitly constrains `A` to `unknown[]`. Using `A` as a type
argument to a generic that requires `T extends unknown[]` must NOT produce
TS2344 — TSC defers the check to conditional type evaluation.

The bug was in `extends_clause_has_constrained_infer_named`: it correctly
detected tuple rest infers (`[...infer T]` → `REST_TYPE` wrapping `INFER_TYPE`)
but missed function rest parameter infers (`...args: infer A` → bare `INFER_TYPE`
annotation on a parameter with `dot_dot_dot_token = true`).

## Files Touched

- `crates/tsz-checker/src/state/type_resolution/constructors.rs` (~15 LOC)
- `crates/tsz-checker/tests/ts2344_infer_conditional_constraint.rs` (2 new tests)

## Verification

- `cargo test -p tsz-checker --test ts2344_infer_conditional_constraint` — 11/11 pass
- `cargo test -p tsz-checker --test generic_call_inference_tests` — 130/130 pass
- `cargo test -p tsz-checker --test conditional_infer_tests` — 37/37 pass
- Conformance snapshot: 12580/12582 (100.0%) — no regressions
