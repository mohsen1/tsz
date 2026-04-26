# fix(checker): emit TS2493/TS2339 for every out-of-bounds element in array destructuring assignment

- **Date**: 2026-04-26
- **Branch**: `fix/destructuring-assignment-tuple-bounds-all`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (conformance)

## Intent

`check_tuple_destructuring_bounds` (in the array-destructuring-assignment
path) emitted TS2493 (single-tuple) and TS2339 (union-of-tuples) only for the
**first** out-of-bounds element, then early-returned. tsc emits one
diagnostic per out-of-bounds target. Removing the early `return;` after the
emit makes the loop continue to flag every remaining element. Fixes
`emitCapturingThisInTupleDestructuring1.ts`.

## Files Touched

- `crates/tsz-checker/src/assignability/assignment_checker/assignment_ops.rs`
  (drop two `return;` statements in `check_tuple_destructuring_bounds`)
- `crates/tsz-checker/tests/tuple_index_access_tests.rs` (two new regression
  tests — TS2493 and TS2339 multi-element destructuring assignment)

## Verification

- `cargo nextest run -p tsz-checker --test tuple_index_access_tests`
  (10/10 pass, including 2 new regression tests)
- `./scripts/conformance/conformance.sh run --filter "emitCapturingThisInTupleDestructuring"`
  (2/2 pass — was 1/2 before)
- `./scripts/conformance/conformance.sh run --filter "destructuring"`
  (162/174 pass — same as before)
