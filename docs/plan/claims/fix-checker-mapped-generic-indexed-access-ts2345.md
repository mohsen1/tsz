# fix(checker): suppress false TS2345 on generic mapped-type indexed access

- **Date**: 2026-04-29
- **Branch**: `fix/checker-mapped-generic-indexed-access-ts2345-resume`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — false-positive elimination)

## Intent

Eliminate the false-positive TS2345 emitted on
`conformance/compiler/mappedTypeGenericIndexedAccess.ts` at
`this.entries[name]?.push(entry)`:

```
Argument of type 'Types[T]' is not assignable to parameter of type 'never'.
```

## Root cause

Inside `addEntry<T extends keyof Types>(name: T, entry: Types[T])`:

```ts
if (!this.entries[name]) {
    this.entries[name] = [];
}
this.entries[name]?.push(entry);
```

The `this.entries[name] = []` assignment was typing the empty array
literal as `never[]` (the evolving-array base in `--strict` mode) even
though the storage slot's declared type was `Types[T][] | undefined`.
After flow narrowing joined the two branches of the `if`, the slot's
narrowed type became `Array<X> | never[]`, then `?.push` resolved
through `evaluate_type_with_env` which subtype-reduces the union of
the two `push` callable shapes. Function types are contravariant in
their parameters, so `(...items: never[]) => number` is the
*supertype* of `(...items: X[]) => number`, and the reduction kept
the `never[]`-element callable. Push's contravariant parameter
collapsed to `never`, producing `Argument of type 'Types[T]' is not
assignable to parameter of type 'never'`.

## Fix

In `crates/tsz-checker/src/types/computation/array_literal.rs`,
extend the empty-array-literal branch so that when the contextual
type is an array (already-handled tuple case generalized), and the
literal is being used to write into a storage slot whose declared
type already pins the element shape, adopt the contextual element
type instead of the evolving-array `never` base. This matches tsc's
`getInferenceContextForType` behaviour at the assignment site.

The new helper `empty_array_in_storage_assignment_context` recognises
direct assignment RHS, compound assignment RHS (`||=`, `??=`, `&&=`),
annotated variable initializers, property declarations / assignments,
and parameter declarations. Generic-call argument positions are
deliberately *not* matched: the contextual type there is a still-
being-inferred type parameter, and adopting it would prevent
`f1<T>(x: T[])(arg: [])` from binding `T = never`.

## Tests

- New `empty_array_in_storage_assignment_adopts_contextual_element`
  unit test pinning the false-positive elimination.
- New `empty_array_in_generic_call_argument_still_drives_inference_to_never`
  unit test guarding the discriminator from regressing
  `co_contra_inference_tests::only_never_candidates_resolves_to_never`.
- `mappedTypeGenericIndexedAccess` conformance test passes 1/1.
- 11243 `tsz-checker` + `tsz-solver` unit tests pass.

## Background

Earlier session (PR #1808, closed) had narrowed the failure surface
and identified the type pipeline that produced the `never` callable
parameter. This PR resumes that work, traces the missing piece (it
was in array-literal contextual typing, not in the call-resolution
pipeline as initially suspected), and ships the fix.
