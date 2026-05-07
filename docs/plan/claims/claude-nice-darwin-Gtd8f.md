# [WIP] fix(checker): canonicalize unparameterized Array in tuple rest position

- **2026-05-07 09:00:00**
- **Date**: 2026-05-07
- **Branch**: `claude/nice-darwin-Gtd8f`
- **PR**: TBD
- **Status**: claim
- **Workstream**: bug-fix (#3988)

## Intent

Fix issue #3988: `[T, ...Array]` (unparameterized `Array` / `ReadonlyArray`
used as a tuple rest element) was being stored as the bare lazy interface
type instead of being canonicalized to `Array<any>` / `ReadonlyArray<any>`.
That produced spurious TS2322 on initialization (`Type 'string' is not
assignable to type 'Array'`) and wrong destructured element types
(`Array | undefined` instead of `any | undefined`). tsc treats
`[string, ...Array]` as `[string, ...any[]]` and emits only the expected
TS18048 for indexed access under `noUncheckedIndexedAccess`.

The canonicalization is keyed on AST shape (TYPE_REFERENCE named
`Array`/`ReadonlyArray` with no type arguments) and applied only in
tuple rest positions when the reference is unshadowed by a local
declaration. Mirrors the existing canonicalization in
`type_literal_checker.rs:308` for the with-args case.

## Files Touched

- `crates/tsz-checker/src/types/type_node.rs` (collapses three
  `let elem_type = self.check(...)` calls into a new
  `check_tuple_element_type` wrapper)
- `crates/tsz-checker/src/types/type_node_helpers.rs` (~60 LOC for the
  new helper)
- `crates/tsz-checker/tests/spread_rest_tests.rs` (~80 LOC; four
  regression tests covering `Array`, `ReadonlyArray`, named tuple rest,
  and the local-shadow negative case)

## Verification

- `cargo test -p tsz-checker --test spread_rest_tests` — 87/87 pass
  (the 4 new `unparameterized_*` tests included).
- `cargo test -p tsz-checker --lib` — 3716/3719 pass; the 3 remaining
  failures are pre-existing on `main` and unrelated to this change.
- `cargo test -p tsz-checker --lib architecture_contract_tests_src` —
  88/88 pass; `types/type_node.rs` stays at 1995 LOC (under the 2000
  ceiling).
