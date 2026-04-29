# fix(checker): suppress false TS2345 on generic mapped-type indexed access

- **Date**: 2026-04-29
- **Branch**: `fix/checker-mapped-generic-indexed-access-ts2345`
- **PR**: #1808
- **Status**: research (deferred)
- **Workstream**: 1 (Diagnostic Conformance — false-positive elimination)

## Intent

Eliminate the false-positive TS2345 emitted on
`conformance/compiler/mappedTypeGenericIndexedAccess.ts` at
`this.entries[name]?.push(entry)`:

```
Argument of type 'Types[T]' is not assignable to parameter of type 'never'.
```

## Reproducer narrowing

The bug fires only with the conjunction of:

1. A class field whose type is a mapped type with optional values:
   `entries: { [T in keyof Types]?: Types[T][] }`.
2. The mapped type-parameter name **collides** with the calling method's
   own generic parameter (both named `T`).
3. The receiver is `this.<field>[arg]` directly (no local alias) and is
   chained through `?.` into a method call.
4. Either a field initializer (`= {}`) or a constructor write
   `this.entries = {}` *plus* an `if (!this.entries[name]) { this.entries[name] = [] }` guard
   precedes the `.push` call.

Each of the following alternatives **eliminates** the diagnostic:

- Rename the mapped parameter (`[T in keyof Types]` → `[K in keyof Types]`).
- Bind to a local first: `const e = this.entries; e[name]?.push(entry)`.
- Pull the field outside the class (top-level `const entries`).
- Drop the assignment-guard pattern (no `entries[name] = []` write).
- Replace `?.push` with `?.indexOf` (works fine).

## Diagnostic trace (rust eprintln-instrumented dist-fast build)

At the point the diagnostic is emitted:

- `error_argument_not_assignable_at`:
  `arg = IndexAccess(Types, T)` (correct — the source argument, generic preserved)
  `param = Intrinsic(Never)` (the bug — `push`'s parameter type is `never`)
- `resolve_call`:
  `func = Callable(CallableShapeId(97))` (a single resolved push signature, **not** a
  union), arg = the same generic indexed access.
- `get_type_of_call_expression_inner`'s `callee_type`:
  `Union(TypeListId(38))` immediately after callee resolution. The Union
  collapses to a single `Callable(97)` somewhere in the chain
  `evaluate_application_type` → `resolve_lazy_type` →
  `resolve_lazy_members_in_union` → `replace_function_type_for_call`,
  losing the per-array-branch element types and ending up with
  `never[]` as the array element.
- `resolve_union_call` is **not** entered, so the bug isn't the union-of-callables
  intersection at `crates/tsz-solver/src/operations/core/call_resolution.rs:712`.

## Hypothesis

The receiver of `push` is being baked into a single `Callable` whose array
element type is the *intersection* of all mapped-type values
(`{a1:true} & {a2:true} & {a3:true}` → collapsed to `never` somewhere in
the union → callable lowering), instead of preserving the indexed access
`Types[T]`. The "name collision + this access + guard" pattern is what
triggers this lowering — the equivalent forms above all preserve
`Types[T]` correctly.

The likely fix lives in one of:

- `evaluate_application_type` / `resolve_lazy_type` /
  `resolve_lazy_members_in_union` /
  `replace_function_type_for_call` — wherever the `Union(38)` callee gets
  collapsed into a single `Callable(97)` with `never` element type.
- `try_mapped_type_param_substitution`
  (`crates/tsz-solver/src/evaluation/evaluate_rules/index_access.rs:1416`)
  — confirm whether it fires for the class-field receiver case; if not,
  identify why the field's type isn't a `Mapped` at the point of indexing.

## Status

Deferred. The reproducer is fully narrowed and the failure surface is
identified, but the actual fix requires deeper traversal of the
checker→solver call-resolution pipeline than this slice budgeted. Next
agent should pick this up with the diagnostic trace above as the
starting point.
