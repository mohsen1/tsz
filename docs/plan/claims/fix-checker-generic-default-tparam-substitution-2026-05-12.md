# fix(checker/solver): substitute prior bindings into generic type-param defaults

- **Date**: 2026-05-12
- **Branch**: `fix/checker-generic-default-tparam-substitution-2026-05-12`
- **Base**: `main`
- **Issue**: [#5878](https://github.com/mohsen1/tsz/issues/5878)
- **Status**: claim
- **Labels**: `bug`, `false-positive`, `type-inference`

## Intent

Closes #5878. Fix the false-positive TS2322 from this repro:

```ts
type Container<T, V = T[]> = { value: T; items: V; };
const c1: Container<string> = { value: "hello", items: ["a", "b"] };
```

`V`'s default `T[]` is currently left symbolic — the solver doesn't
substitute `T → string` before using `T[]` as `V`'s effective type,
so `["a", "b"]` gets compared against the unbound `T[]` and fails
assignability.

## Approach (preliminary, pending Explore-agent diagnosis)

The structural rule:

> "When a type-parameter slot is filled from its declared default, the
> substitution map for prior bound parameters MUST be applied to the
> default's TypeId before it is used as the effective type for the
> slot."

Concretely: in the default-application path (in
`crates/tsz-solver/src/instantiation/` — exact site TBD), the
default `TypeId` (here, `T[]`) is currently treated as final.
Instead, it must be `instantiate`d against the partial binding
`(T → string)` to produce `string[]`.

This mirrors how *constraints* are handled (`<T, U extends T>` works
because the constraint resolution substitutes prior bindings). The
fix should reuse that substitution machinery rather than
introducing a parallel path.

## Out of scope

- Defaults that involve `keyof`, conditionals, or mapped types
  (those compose the same substitution but the present bug is the
  simpler "TypeParameter → bound" reference). Will land in a
  follow-up if the basic fix is too narrow.
- Defaults of constructor signatures vs. type aliases — the rule
  is the same, but the fix may need to be applied in two sibling
  call sites.

## Verification plan

- Unit test in `crates/tsz-checker/tests/` (new file
  `generic_tparam_default_substitution_tests.rs`) that locks the
  rule with at least two name choices (`T`/`U`, `K`/`V`) per
  CLAUDE.md §25.
- Targeted conformance smoke (`scripts/conformance/conformance.sh
  run --filter "default"` after the fix).
- Full `cargo nextest run -p tsz-checker` + `-p tsz-solver` regression
  guards.

## Risk

Medium. Substitution into defaults touches the instantiation hot path;
need to ensure we don't widen substitution to non-default slots and
break unrelated tests. Diagnose-first via Explore agent, then
implement the smallest-scope fix.

## Followups (not in this PR)

If the Explore agent surfaces a parallel set of "default of CALLABLE
type-param" sites with the same bug, those will land in a sibling
PR rather than expanding this one.
