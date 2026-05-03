---
title: typeAssertionToGenericFunctionType — eager generic-shape inference from contextual target
branch: fix/grind-1777801646
status: deferred
date: 2026-05-03
---

# Investigation summary

`TypeScript/tests/cases/compiler/typeAssertionToGenericFunctionType.ts`
emits a spurious TS2352 on the assertion
`< <T>(x: T) => T > ((x: any) => 1)`.

We display the source as `<T>(x: T) => number` — the lambda's type was
**re-shaped to be generic** (gaining a `<T>` parameter) when the
contextual target `<T>(x: T) => T` was applied. The lambda's explicit
`x: any` annotation should pin the parameter to `any`, but the
contextual-typing path picked up the target's `T` and assigned it as
the parameter's type. The literal `1` return then tries to satisfy `T`
in the target while showing as `number` in the source's display.

## Root cause boundary

For arrow-function expressions inside an `as` / `<T>` assertion, the
checker is treating the contextual target's type-parameter list as if
the lambda itself were generic with the same `<T>`. tsc instead treats
the assertion as a sink: the lambda is checked against the contextual
type via assignability, but its OWN signature comes from the explicit
parameter annotations, NOT from the target's type-parameter list.

## Where to look next

`crates/tsz-checker/src/dispatch.rs` around the TYPE_ASSERTION arm
(line ~1019), where `request.contextual(asserted_type).assertion()`
is applied to the lambda. The contextual-arrow-typing pipeline likely
generalises the parameter type from the target's type parameter when
the lambda's annotation is `any`. Either:

1. Block contextual-type-driven param re-shaping when the lambda has
   an explicit type annotation.
2. Strip the target's outer `<T>` quantifier before piping the
   contextual type into the lambda body — assertion semantics are
   "assert the body produces a value matching the asserted type after
   discarding free generics".

Other small candidates investigated this iteration that hit the same
"display preserves alias / resolves type-args" gap:
- `reverseMappedTypeContextualTypeNotCircular.ts` — target shows as
  `Selector<S, T["editable"]>` instead of tsc's `Selector<unknown, {}>`.
- `mappedTypeUnionConstrainTupleTreatedAsArrayLike.ts` — source shows
  expanded `[T[0] extends string ? boolean : null]` instead of the
  alias `HomomorphicMappedType<T>`.
- `assignmentCompatWithGenericCallSignatures4.ts` — we emit two errors
  where tsc emits one (the dual is suppressed once the first fires).
