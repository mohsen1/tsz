# fix(checker): keep `[]` at `never[]` for `&&=` RHS

- **Date**: 2026-04-29
- **Branch**: `fix/checker-empty-array-and-and-equals`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Conformance)

## Intent

PR #1849 generalized the empty-array storage-context widening to all
three logical-assignment operators (`||=`, `??=`, `&&=`). For `||=` and
`??=` that is correct — those operators replace a falsy/nullable LHS
with the RHS, so widening `[]` to the LHS element type matches user
intent. For `&&=` it is **not** correct: tsc keeps the RHS empty array
at `never[]` so a chained `.push(x)` on the resulting
`(falsy LHS) | typeof []` reports TS2345 ("Argument of type 'X' is not
assignable to parameter of type 'never'"). Without that distinction
tsz silently accepted the push, which regressed `logicalAssignment6.ts`
and `logicalAssignment7.ts`.

This PR removes `AmpersandAmpersandEqualsToken` from the storage-context
match in `empty_array_in_storage_assignment_context`, restoring tsc
parity for `&&=`.

## Files Touched

- `crates/tsz-checker/src/types/computation/array_literal.rs`
  - Drop `&&=` from the binary-expression case (≈8 LOC, plus a
    rationale comment).
  - Two new unit tests:
    `empty_array_rhs_of_and_and_equals_keeps_never_element` (the
    regression case) and
    `empty_array_rhs_of_or_or_equals_adopts_lhs_element` (counter-test
    locking the kept-as-is `||=`/`??=` behavior).

## Verification

- `cargo nextest run -p tsz-checker` (5712 tests pass).
- `bash scripts/conformance/conformance.sh run` net +2 vs previous
  snapshot: `logicalAssignment6.ts` and `logicalAssignment7.ts` move
  from FAIL back to PASS (they were in the 3-regression set introduced
  by #1849).
- Local repro `(results &&= (results1 &&= [])).push(100)` now matches
  tsc: TS2532 + TS2345; the `||=` and `??=` siblings remain clean.
