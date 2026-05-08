# Document HOFI gap with self-referential constraints (genericFunctionInference1)

- **Date**: 2026-05-08
- **Branch**: `claude/brave-thompson-BEbFd`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes — investigation)

## Intent

While picking a random conformance failure with `scripts/session/quick-pick.sh`,
the picker selected `compiler/genericFunctionInference1.ts` (fingerprint-only
TS2345 mismatch). Investigation traced eight extra `TS2345` diagnostics to a
single root cause: tsz lacks higher-order function inference (HOFI) for the
specific shape `<T extends C(T)>(x: T) => T` (a generic function with a
self-referential constraint) when passed as an argument to another generic
function whose parameter type is `(...args: A) => B`.

This claim does not land a fix for HOFI itself — that is a multi-week project
spanning solver inference resolution and how the outer call's result type is
materialized. Instead, it ships:

1. A **regression test** locking in the existing correct behavior for
   non-self-referential constraints (e.g. `<T extends string>(x: T) => T`
   passed to `pipe`) so future inference refactors don't break the cases that
   already work.
2. An **ignored test** capturing the desired behavior for self-referential
   constraints, so future agents working on HOFI have a runnable baseline.
3. A precise root-cause writeup so the next agent can resume from the analysis
   point instead of re-deriving it.

## Root cause (one-sentence form)

When a generic source function whose type parameter has a self-referential
constraint (`T extends { value: T }`) is passed to a generic target whose
parameter is `(...args: A) => B`, tsz's `constrain_types_impl` Function/Function
branch instantiates `T` to a fresh inference placeholder `__infer_src_X`,
drops the constraint because it contains the placeholder
(`crates/tsz-solver/src/operations/constraints/walker.rs:1306`), seeds
`__infer_src_X` as the only candidate for `A_pipe`/`B_pipe`, and at resolution
time both placeholders fall back to `unknown`
(`crates/tsz-solver/src/inference/infer_resolve.rs:553`). The outer call then
materialises as `(...args: [x: unknown]) => unknown`, against which the
generic source argument is no longer assignable — producing the observed
`TS2345` diagnostics on lines 20, 21, 33, 34 of the test (and four related
HOFI-without-self-ref-constraint extras on lines 24, 25, 29, 30 that share
the same family).

## Reproducer (minimal)

```ts
declare function pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B;
declare function foo<T extends { value: T }>(x: T): T;

const f = pipe(foo); // tsc: ok, tsz: TS2345 (spurious)
```

`tsc` infers `f: <T extends { value: T; }>(x: T) => T`. `tsz` infers
`f: (...args: [x: unknown]) => unknown` and rejects the call.

## Files touched

- `docs/plan/claims/claude-brave-thompson-BEbFd.md` (this file)
- `crates/tsz-checker/tests/generic_call_inference_tests.rs` (regression +
  ignored doc test)

## Verification

- `cargo nextest run -p tsz-checker --test generic_call_inference_tests`
- `scripts/session/verify-all.sh --quick`
