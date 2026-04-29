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

## Investigation Findings

- The diagnostic-display alias resolution lives in
  `tsz-solver`'s `find_type_alias_by_body` lookup, fed by
  `register_resolved_type` → `set_body(def_id, type_id)` in
  `crates/tsz-checker/src/context/def_mapping.rs:921`.
- `set_body` keys on the resolved alias body's `TypeId`. For
  `A<T> = (T & U)`, the lowered body type is the same intersection
  `T & U` whether or not the source is parenthesized — so the alias *should*
  be findable. The bug is upstream of `set_body`.
- The alias body is lowered through `type_node_resolution.rs::ensure_type_alias_resolved`
  (see line ~650 in this file). Need to trace whether the inner unwrapping
  of `PARENTHESIZED_TYPE` happens BEFORE or AFTER the `set_body` call. If
  before, then the alias's body type is the inner intersection — and any
  later `format_type` for an *application* of A (i.e.
  `Application(A_def, [{x:number}])`) ends up evaluating to a plain
  intersection without the alias-application wrapper.
- Likely root cause: when applying `A<{x:number}>`, tsz instantiates the
  alias body and discards the `Application` wrapper / alias-symbol marker
  during evaluation, so the resulting `TypeId` displays as the structural
  intersection. tsc preserves the application form for display.
- Compare: tsc's printer keeps the `aliasSymbol` and `aliasTypeArguments`
  metadata on the resulting type (see tsc's `getAliasSymbol` /
  `aliasInstantiations` paths). tsz's solver may have analogous metadata
  but isn't propagating it through the application instantiation.

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
