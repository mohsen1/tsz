# fix(solver): unpack `[] | [X]` rest tuple unions to optional fixed params

- **Date**: 2026-04-26
- **Branch**: `fix/solver-bivariant-params-only-method-compat`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 — Diagnostic Conformance / TS2416 false positives (foundation work)

## Intent

The lib's `Iterator.next` / `AsyncIterator.next` declare their parameter as
`...[value]: [] | [TNext]` — a rest binding whose tuple-list type is a union
of fixed-length tuples. tsc treats this as structurally equivalent to
`(value?: TNext)` for signature compat. tsz's `unpack_tuple_rest_parameter`
only handled single fixed tuples (`[A, B, C]`); it left the union form alone,
so the function-rules path could not reach the parameter-by-parameter
comparison code that already handles optional/rest properly.

This PR teaches `unpack_tuple_rest_parameter` to recognize the
prefix-aligned union pattern and flatten it into a list of optional fixed
parameters. Mirrors tsc's structural equivalence for this lib idiom.

## Files Touched

- `crates/tsz-solver/src/type_queries/data/accessors.rs`
  — new `unpack_union_of_prefix_tuples` helper (~80 LOC) and a hook in
  `unpack_tuple_rest_parameter` to call it before the existing single-tuple
  fall-through. Bails on non-tuple union members, on tuples with rest tails,
  and on disagreeing positions (so `[X] | [Y]` keeps the rest type).
- `crates/tsz-solver/src/tests/type_queries_function_rewrite_tests.rs`
  — three new tests pinning the behavior: `[] | [X]` → one optional,
  `[X] | [X, Y]` → required + optional, `[X] | [Y]` → preserved as rest.

## Verification

- `cargo nextest run -p tsz-solver --lib type_queries_function_rewrite` — 6 pass
  (3 existing + 3 new)
- `cargo nextest run -p tsz-solver --lib` — 5517 pass
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` — full suite

## Notes on `customAsyncIterator.ts`

The original target test for this work, `customAsyncIterator.ts`, does **not**
flip with this change alone. End-to-end tracing showed the failure surfaces
*before* parameter unpacking is reached: `check_function_subtype_impl` first
runs `check_return_compat` on the Promise return types, and that comparison
returns False for `Promise<IteratorResult<T, any>>` vs
`Promise<IteratorResult<T, void>>` — a separate Application/instantiation
comparison issue. This PR is a foundation (correctly handles the lib's
parameter shape) so the eventual return-type fix can land cleanly without
re-fighting the parameter side.
