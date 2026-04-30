# fix(solver): preserve nullable-union arg inference for repeated naked T

- **Date**: 2026-04-30
- **Branch**: `fix/checker-inference-nullable-common-base-ts2345`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — false-positive elimination)

## Intent

Eliminate the false-positive TS2345 emitted on
`conformance/compiler/inferenceOfNullableObjectTypesWithCommonBase.ts`
at lines 29 and 34, where a nullable-union second argument to
`equal<T>(a: T, b: T)` was being skipped during inference and
resolving to `never` against an over-narrow `T`.

## Root cause

`resolve_generic_call_inner`
(`crates/tsz-solver/src/operations/generic_call/resolve.rs`) has a
"first-direct-primitive-mismatch" optimization for repeated naked
type-parameter parameters: for `f<T>(a: T, b: T)` called with `(1,
"")`, it skips adding `""` as a candidate for `T` so `T = number` and
`""` is reported against the inferred `T` (matching tsc).

The skip fires whenever the later argument's primitive base differs
from the first candidate's. For
`equal(v as "a", v as "b" | undefined)`:

- arg 0 candidate: `"a"` (primitive base = `string`).
- arg 1 type: `"b" | undefined` (a union; tsz's
  `primitive_base_of` returns `None` for unions, so the `current_base
  != Some(first_base)` check trips and the skip fires).

The second arg is dropped from inference. `T` resolves to `"a"`,
nullable is added back per tsc's `getCommonSupertype`, and `T = "a" |
undefined`. The argument check then narrows `"b" | undefined` against
`"a" | undefined`, producing `never` for the disjoint literal and a
false TS2345.

tsc does not skip the second argument here. Its `getCommonSupertype`
strips nullable members from each candidate (`primaryTypes`), runs
`reduceLeft`, and adds nullable back via `getNullableType`. Both
literals contribute to `primaryTypes`, the tournament picks `"a"`
(first-wins), and the nullable add-back yields `T = "a" | undefined`
— which is wide enough to accept `"b" | undefined` because the
nullable union members aren't structurally rejected.

## Fix

In the first-direct-primitive-mismatch block, bypass the skip when
the later argument's type is a union containing at least one
nullable member. Such unions go through tsc's nullable-stripping path
in BCT and should not be dropped before that.

```rust
let arg_is_nullable_union =
    if let Some(TypeData::Union(list_id)) =
        self.interner.lookup(source_for_inference)
    {
        self.interner.type_list(list_id).iter().any(|m| m.is_nullable())
    } else { false };
if !arg_is_nullable_union
    && let Some(&var) = var_map.get(&contextual_target_type)
    // … existing first-wins skip conditions …
```

## Tests

- New `nullable_union_second_arg_does_not_skip_inference` unit lock
  in `crates/tsz-checker/tests/co_contra_inference_tests.rs`.
- Existing `only_never_candidates_resolves_to_never`,
  `test_pick_utility_inference`, `test_record_utility_inference`,
  `ts2345_generic_call_parameter_display_preserves_instantiated_alias_name`
  guards still pass.
- `inferenceOfNullableObjectTypesWithCommonBase` conformance test:
  1/1.
- 11247 `tsz-checker` + `tsz-solver` lib tests pass.

## Why this slice is small

The failure surface looked deep at first — TS2345 with `'never'` as
the parameter type pointed at the call-resolution pipeline — but
tracing the candidate flow revealed the problem was a single
optimization in the generic-call orchestrator dropping a candidate
that BCT would have absorbed correctly. The fix is a one-line
condition on a single skip branch; no tsc-equivalent
`getWidenedLiteralType` rework was required.
