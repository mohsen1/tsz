# fix(checker): preserve type alias name in TS2352 display when alias body is parenthesized

- **Date**: 2026-04-29
- **Branch**: `fix/checker-paren-alias-symbol-display-parity`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance and Fingerprints)

## Intent

`parenthesisDoesNotBlockAliasSymbolCreation.ts` is a fingerprint-only TS2352
failure: the alias name is lost in the diagnostic when the alias body is
syntactically parenthesized.

```ts
export type A<T> = (
    T & InvalidKeys<"a">
);
export const a = null as A<{ x: number }>;  // line 14
```

- tsc:  `Conversion of type 'null' to type 'A<{ x: number; }>' may be a mistake...`
- tsz:  `Conversion of type 'null' to type '{ x: number; } & InvalidKeys<"a">' may be a mistake...`

The alias `A` is unwrapped to its expanded body in tsz's display because the
alias-name registration path doesn't survive the `PARENTHESIZED_TYPE` wrapper
on the body.

The same expansion happens for `A2<T> = ( { [P in K]?: never } )` — a
parenthesized mapped-type body. So the fix must apply uniformly to all
`PARENTHESIZED_TYPE` bodies, not just intersections.

## Investigation Findings (iter 2)

- TS2352 emission path: `dispatch.rs::~1098` → `error_type_assertion_no_overlap`
  in `crates/tsz-checker/src/error_reporter/generics.rs:485`.
- Source/target rendering goes through `format_type_assertion_overlap_display`
  (same file, `:262`). Notable branch at `:279`:

  ```rust
  let evaluated = self.evaluate_type_with_env(type_id);
  if let Some(alias_origin) = self.ctx.types.get_display_alias(evaluated)
      && let Some(app) = type_application(self.ctx.types, alias_origin)
      && let Some(def_id) = lazy_def_id(self.ctx.types, app.base)
      && let Some(def) = self.ctx.definition_store.get(def_id)
      && def.kind == TypeAlias
      ...
  { /* render as alias */ }
  ```

- The display-alias mapping is supposed to be installed by
  `evaluate_application` in
  `crates/tsz-solver/src/evaluation/evaluate.rs:920-940`:
  when an `Application(A, [args])` evaluates to a different `result`
  (e.g. the inner intersection), it stores
  `display_alias[result] = Application` so the formatter can recover
  the alias name.
- Hypothesis (still to verify): for `A<T> = (T & U)`, the lowering may
  return the *evaluated* intersection directly (skipping the
  `Application` form), or `evaluate_application` is short-circuiting
  before reaching the `store_display_alias` call. Need a debug print of
  the `TypeId` returned by `lower_type` for the `A<{x:number}>` node and
  whether it's `TypeData::Application(...)` or a structural type.
- For `A2<T> = ( { [P in K]?: never } )` (parenthesized mapped-type),
  the same symptom occurs in the second TS2352 (`A2<...>` not preserved).

## Next Steps

1. Add a `tracing::debug` in `error_type_assertion_no_overlap` that prints
   the `TypeData` variant of `target_type` and `get_display_alias(target)`
   for the failing test, then run the conformance test and inspect.
2. If `target_type` is *not* an `Application`, the bug is upstream in
   `lower_type` for `TYPE_REFERENCE` whose alias body is `PARENTHESIZED_TYPE`
   wrapping `INTERSECTION_TYPE` / `MAPPED_TYPE` — need to ensure the
   lowering keeps the `Application` wrapper.
3. If `target_type` IS an `Application` but `get_display_alias` returns
   `None`, the `evaluate_application` path is missing the
   `store_display_alias` call — likely because of the parenthesized inner
   shape. Trace the `result != original_type_id` and `has_param_args`
   conditions.

## Files Likely Involved

- `crates/tsz-checker/src/types/type_node_resolution.rs` (alias body lowering)
- `crates/tsz-solver/src/diagnostics/format/...` (type formatter — alias name
  lookup path)
- `crates/tsz-solver/src/types.rs` and the `Application` evaluator
  (preserve `Application(A, T)` for display when A is a non-conditional
  alias whose body is structurally simple)

## Verification

- New unit-test lock with three cases: parenthesized intersection body,
  parenthesized mapped-type body, and a control case where the body is
  not parenthesized (should already pass).
- `./scripts/conformance/conformance.sh run --filter "parenthesisDoesNotBlockAliasSymbolCreation" --verbose`
  — expect TS2352 first-line message to render the alias name.
- Targeted regression: search for similar fingerprint-only tests where the
  expected display has `<aliasName>(<args>)` and tsz expands to the body —
  this fix likely flips multiple tests at once.
