---
title: nonnullAssertionPropegatesContextualType — method-overload contextual return inference deferred
branch: fix/grind-1777798504
status: deferred
date: 2026-05-03
---

# Investigation summary

`TypeScript/tests/cases/compiler/nonnullAssertionPropegatesContextualType.ts`
fails with an extra TS2740 ("Type 'Element' is missing the following
properties from type 'SVGRectElement'…"). tsc emits no errors here:

```ts
let rect2: SVGRectElement = document.querySelector('.svg-rectangle')!;
```

## Root cause boundary

The contextual-return inference works correctly when the call target has
a **single** overload, even through `!`. Reduced repros confirm:

- Standalone function with two overloads (TagNameMap-keyed + generic
  `<E = Element>`) and `!`: PASSES.
- Method on interface with two overloads + `!`: FAILS (E falls back to
  the default `Element` instead of being inferred from contextual
  `SVGRectElement`).
- Method on interface with two overloads, no `!` (LHS is `T | null`):
  PASSES.

So the bug is the combination of (a) method-receiver overload resolution
and (b) `!`-narrowed contextual return type. Either changing the call
to a standalone function or removing `!` makes inference work.

## Failed approach

Widening the inner contextual type from `T` to `T | null | undefined`
when descending through `NON_NULL_EXPRESSION` (in `dispatch.rs`)
**regresses** the single-overload case (test-nn6), because the existing
"assign `T` to `E | null`" inference rule infers `E = T` when the source
is `T` but bails when the source is `T | null | undefined`. So the
widening is correct conceptually but exposes a separate gap in the
contextual-return matcher: it cannot reduce a `T | null | undefined`
contextual against `E | null` to infer `E = T`.

## Where to look next

`crates/tsz-checker/src/types/computation/call_inference.rs` —
`compute_round2_contextual_types` and the per-overload
`collect_return_context_substitution` codepath. The method-call
overload resolver appears not to feed the LHS contextual type into
later overloads after rejecting an earlier one whose constraint matched
on the contextual return shape (e.g. `keyof SVGElementTagNameMap`
inferring `K = "rect"` from the contextual `SVGRectElement`). The
inference state from the rejected overload likely leaks.

A working fix would either:
1. Make the contextual-return matcher able to reduce
   `T | null | undefined` against `E | null` (covariant subtyping), so
   the simple `!`-widening in `dispatch.rs` would work end-to-end.
2. Reset per-overload inference state cleanly on rejection so a
   downstream overload starts from the LHS contextual without
   leftover bindings.

# Other deferred candidate from this iteration

`TypeScript/tests/cases/compiler/maxConstraints.ts` — error message
displays target as `Comparable<number>` instead of `Comparable<1 | 2>`.
Inference widens the literal-type union to `number` before reporting
the constraint failure. Fix needs to preserve literal types in the
expected-type render path for generic constraint mismatches.
