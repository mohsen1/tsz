# fix(solver,checker): tuple-arity mismatch takes priority over inner element mismatch

- **Date**: 2026-04-26 20:16:50
- **Branch**: `fix/solver-tuple-arity-mismatch-priority`
- **PR**: #1470
- **Status**: ready
- **Workstream**: 1 (Conformance — fingerprint parity, Big3 / TS2322 / TS2345)

## Intent

When a closed source tuple has more elements than a closed target tuple,
the solver was returning a `TupleElementTypeMismatch` for the first
type-incompatible element instead of the `TupleElementMismatch` arity
failure. The checker's array-literal elaboration then anchored a TS2322
on the inner element with a misleading message. `tsc` instead reports
the call-site TS2345 with the outer `Source has N element(s) but target
allows only M.` sub-message and does not drill into individual source
elements.

This change:

1. Makes `SubtypeChecker::explain_tuple_failure` short-circuit to
   `TupleElementMismatch` when both source and target are closed and
   source has strictly more elements than target. The returned reason
   then reflects what the relation actually rejected.
2. Drops the symptom-side iteration in
   `try_elaborate_array_literal_mismatch_from_failure_reason` for the
   `TupleElementMismatch` branch. tsc never drills into a specific
   source element on tuple arity failures — the arity sub-message is
   the diagnostic. Returning `false` here lets the outer call-error
   path render the proper TS2345 with related info.

## Files Touched

- `crates/tsz-solver/src/relations/subtype/explain.rs` (+15 LOC)
- `crates/tsz-checker/src/error_reporter/call_errors/elaboration_array_mismatch.rs` (-54 LOC)
- `crates/tsz-solver/tests/compat_tests.rs` (+125 LOC, regression test)

## Verification

- `cargo nextest run --package tsz-solver --lib -E 'test(test_explain) | test(test_tuple) | test(tuple_subtype) | test(tuple_assignability)'`
  → 154 passed
- `cargo nextest run --package tsz-checker --lib` → 2918 passed
- `cargo nextest run --package tsz-solver --lib` → 5521 passed
- Conformance: `destructuringParameterDeclaration3ES5.ts` flips from
  `TS2322 test.ts:27:11 ...string[][]...` to the correct
  `TS2345 test.ts:27:4 Argument of type ...` (only remaining delta is
  a literal vs widened-boolean display — separate fingerprint issue).
- Full conformance: net +1 (3 PASS improvements, 2 unrelated stale-baseline
  drift entries already failing on plain main).
