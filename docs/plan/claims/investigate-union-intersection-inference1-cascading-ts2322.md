# investigate(checker): cascading TS2322 missing on `assign(...)` LHS when wrapper has TS2769

- **Date**: 2026-05-02
- **Branch**: `investigate/union-intersection-inference1-cascading-ts2322`
- **PR**: TBD (handoff doc, no code change yet)
- **Status**: claim
- **Workstream**: 1 (Conformance — fingerprint-only test
  `unionAndIntersectionInference1.ts`)

## Test under investigation

`TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/unionAndIntersectionInference1.ts`

Test failure mode: `fingerprint-only`. The error-code multiset matches
tsc; the diff is exactly one missing fingerprint:

```
missing-fingerprints:
  - TS2322 test.ts:97:7 Type '{ func: <T>() => void; }' is not assignable
                        to type '(() => void) & { func: any; }'.
extra-fingerprints: []
```

Source (lines 96–98 in tsc-stripped numbering):

```ts
const func = <T>() => {};
const assign = <T, U>(a: T, b: U) => Object.assign(a, b);
const res: (() => void) & { func: any } = assign(() => {}, { func });
```

## What both compilers see

Both tsz and tsc emit the same TS2769 inside the wrapper body — the
callsite `Object.assign(a, b)` cannot satisfy the
`assign<T extends {}, U>(target: T, source: U): T & U` overload because
`a: T` is generic and unconstrained, so the constraint
`T extends {}` is unmet:

```
TS2769 test.ts:96:52 No overload matches this call. […]
```

(In the raw on-disk file with the `// @target: es2015` directive line
this is column 52 of line 97.)

## What tsc additionally emits — and tsz does not

tsc *also* reports a top-level assignment mismatch on the
declaration of `res` at the same statement, column 7 (the identifier):

```
TS2322 test.ts:97:7 Type '{ func: <T>() => void; }' is not assignable
                    to type '(() => void) & { func: any; }'.
```

Two surprising facts about that message:

1. The displayed source is **only the U side** of `T & U` — the
   `{ func: <T>() => void; }` shape — not the inferred intersection
   `(() => void) & { func: <T>() => void; }`.
2. The mismatch is structural: a non-callable object literal cannot
   satisfy the call-signature half of `(() => void) & …`.

## Why tsz drops it

tsz's overload-resolution path inside the wrapper returns `TypeId::ERROR`
(or a generic-tainted form) when no overload matches and the synthetic
return cannot be derived. The outer arrow `<T,U>(a, b) => Object.assign(a,b)`
therefore infers a return type that, by the time the outer call
`assign(() => {}, { func })` is checked, propagates as `any` / `ERROR`.
Both are bivariant against the annotation
`(() => void) & { func: any }`, so the assignment passes silently.

Trace of tsz's diagnostic surface for the test:

```
TS2322 line 22:5  ✓ matches tsc 21:5
TS2322 line 26:5  ✓ matches tsc 25:5
TS2322 line 50:4  ✓ matches tsc 49:4
TS2769 line 97:52 ✓ matches tsc 96:52   (raw vs stripped numbering)
        line 98:7 ✗ MISSING — tsc emits TS2322 here, tsz does not
```

## Where the fix likely lives

Two layers, depending on the chosen scope:

1. **`crates/tsz-solver/src/operations/core/call_resolution.rs`** —
   `resolve_callable_call`: when overload resolution fails for a
   non-Object.assign-style 2-arg `<T extends {}, U>(target: T, source:
   U): T & U`, currently we return `CallResult::Failure(ERROR)`. tsc's
   recovery path keeps a syntactic `T & U` (substituted with the
   *pre-failure* candidate bindings) so downstream assignment checks
   still see a structural type.

2. **`crates/tsz-checker/src/types/computation/call/inner.rs`** —
   `get_type_of_call_expression_inner` consumes `CallResult` and
   currently propagates `ERROR` upward when the failure is
   constraint-only. The outer arrow's body return-type inference then
   rolls that up into the wrapper's signature. A targeted change here
   is to keep the wrapper's *declared structural return* (the syntactic
   `T & U` from the matched overload's return type, with substitution
   applied) when the only failure is a constraint-violating generic
   argument.

## What to try next

- Reproduce in a one-liner unit test against `tsz_solver::CallEvaluator`
  with `Object.assign<T extends {}, U>(t: T, s: U): T & U` as the
  callable and `T = U = unknown` (both unbound). Confirm the current
  output is `ERROR` rather than `T & U`.
- See `instantiate_function_shape` and the constraint-violation branch
  in `resolve_callable_call`. If we can return a substituted `T & U`
  (with `T` bound to its constraint `{}` and `U` left as `unknown`),
  the wrapper's inferred return becomes a structural intersection,
  which the outer assignment will then check.
- Risk: this could introduce false positives on call sites where
  overload resolution genuinely should poison downstream. Add a guard
  so the recovered structural return is *only* used when the failure
  is constraint-on-generic and the return type itself is concrete (an
  intersection / object literal / class instance — not a unknown
  type-variable).

## Context this hand-off preserves

- Conformance baseline at hand-off time: 12344/12582 (98.1%).
- `cargo test -p tsz-checker --lib`: 3146 pass, 0 fail.
- The same Object.assign-in-wrapper construct shows up in several other
  conformance tests (search the corpus for
  `Object.assign(a, b)` inside an arrow with generic params); a fix
  here likely flips more than just `unionAndIntersectionInference1`.

## Why this is a hand-off, not a fix

The corrective change touches the solver's call-resolution invariants
and the checker's call-result consumption. Either layer's change
ripples through the overload-resolution suite and the
`call_architecture_tests` lock-set. A safe fix needs:

1. A new `CallResult` variant (or a flag on the existing `Failure`
   variant) that carries the structural fallback return.
2. New regression tests in the solver lock-set
   (`call_resolution_regression_tests.rs`).
3. A full conformance pass to confirm no regressions on the wider
   `Object.assign` / generic-overload corpus.

That work doesn't fit a single 25-minute loop iteration, so this
investigation note is the iteration's artifact: it preserves all the
context (positions, codes, displayed types, suspect code paths,
proposed shape of the fix, risk notes) so the next agent can pick up
where this trail goes cold.
