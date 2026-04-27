**2026-04-27 04:49:00** — fix(solver): pair last source overload for inference when target has 1 sig

Branch: `fix/solver-overload-inference-arrayfrom-20260427-0449`

Scope: Targeted fix in `tsz-solver`'s constraint walker to mirror tsc's
`inferFromSignaturesOfType` pairing behavior. When the constraint walker
needs to infer through a Callable→Callable structural match where the
target has 1 signature and the source has multiple, fall back to the last
source signature when the strict assignability pre-check finds no match —
but only when the source overload set is homogeneous in arity, so we don't
mis-pair semantically distinct overloads (e.g. `Callback<T>`'s
`(null, T)` vs `(Error, null)`).

Conformance impact: closes the spurious-TS2769 cluster on
`Array.from(arr.values())` family calls — partial fix for
`TypeScript/tests/cases/compiler/arrayFrom.ts` (1-arg overloads now infer
through the inheritance-merged `[Symbol.iterator]` overload set;
2-arg variant still needs the separate Array.from declaration-merging fix).

Tests: `crates/tsz-checker/tests/call_resolution_regression_tests.rs::inheritance_merged_overload_pairs_last_source_sig_for_inference`.
