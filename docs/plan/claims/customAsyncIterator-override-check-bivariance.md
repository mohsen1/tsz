---
title: customAsyncIterator — override check fails despite signatures being directly assignable
branch: fix/grind-1777798894
status: deferred
date: 2026-05-03
---

# Investigation summary

`TypeScript/tests/cases/compiler/customAsyncIterator.ts` emits a spurious
TS2416 ("Property 'next' is not assignable to the same property in base
type"). The reduced shape:

```ts
class ConstantIterator<T> implements AsyncIterator<T, void, T | undefined> {
    async next(value?: T): Promise<IteratorResult<T>> { ... }
}
```

We report:
- Source: `(value?: T | undefined) => Promise<IteratorResult<T, any>>`
- Target: `(..._: [] | [T | undefined]) => Promise<IteratorResult<T, void>>`

These two function types are **directly assignable** when written as
freestanding `Sig1` / `Sig2` aliases (verified by reproduction). The
override-check path (TS2416) fails where assignability succeeds, so the
method-context comparison is using a stricter relation than the
freestanding case it claims to be checking.

## Where to look next

`crates/tsz-checker/src/classes/class_checker_compat.rs` (`check_member`
or equivalent) and the override-context relation. Likely candidates:

1. The override check is invoking a strict-arity / strict-rest variant
   of signature comparison rather than the regular subtype check, and
   not normalizing `(value?: T)` against `(..._: [] | [T | undefined])`.
2. `IteratorResult<T, any>` vs `IteratorResult<T, void>` may be
   compared with TReturn locked nominally in the method-context
   comparison, even though `any` is unconditionally assignable to
   `void` everywhere else.

A working fix should either:
1. Reuse the same signature relation as freestanding assignment.
2. Or normalize destructured-rest tuples down to positional
   parameters before override comparison.
