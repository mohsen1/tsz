---
name: JSX single-callable child body-level TS2322 elaboration
status: claimed
timestamp: 2026-05-03 09:30:21
branch: fix/checker-jsx-single-callable-child-body-elaboration
---

# Claim

Workstream 1 (Diagnostic Conformance) — when a JSX body child is a function
expression and the children prop type is a single callable, TS2322 should
elaborate at the body return expression rather than the whole-callable
mismatch on the function.

## Problem

For `<C>{p => "y"}</C>` where `C` has `children: (x: this) => "x"`, tsc
reports:

  TS2322 at `"y"`: `Type '"y"' is not assignable to type '"x"'`.

Tsz reported:

  TS2322 at `p => "y"`:
    `Type '(p: LitProps<"x">) => "y"' is not assignable to type
     '((p: LitProps<"x">) => "y") & ((x: LitProps<"x">) => "x")'`.

The JSX-children path was using
`check_assignable_or_report_at_exact_anchor_without_source_elaboration`,
which skips `try_elaborate_assignment_source_error`. For function-expression
children this prevented the body-level diagnostic from surfacing.

## Fix

In `check_jsx_single_child_assignable`, when the child node is an
`ARROW_FUNCTION` or `FUNCTION_EXPRESSION` and the children target type is a
**single callable** (not a union or intersection in the original prop
declaration, and structurally callable after lookup), route through
`check_assignable_or_report_at_exact_anchor` so the elaboration helper
runs. All other shapes — JSX elements, primitives, union/intersection
target types — keep the non-elaborating path.

Capture the union/intersection-ness of the original `children_type` *before*
it gets contextually narrowed at the call site: `Cb | Cb[]` may collapse to
just `Cb` after contextual selection, but tsc still reports the
whole-callable mismatch on a target that started as a union. The
`children_type_is_originally_compound` parameter threads this signal through
to the elaboration check.

## Tests

- New: `jsx_single_callable_child_body_mismatch_elaborates_at_return`
- Existing test guard preserved:
  `jsx_union_children_single_child_emits_ts2322_without_return_type_elaboration`
  (union target keeps whole-callable mismatch).
- All 235 JSX unit tests pass.

## Conformance impact

Net **+3** vs current main:
- `getAndSetNotIdenticalType2.ts`
- `objectCreate-errors.ts`
- `autoAccessorDisallowedModifiers.ts`

`jsxChildrenGenericContextualTypes.tsx` flips the line-21 fingerprint to
match tsc; lines 20 and 22 remain as separate display-policy gaps
(attribute-path source elaboration and number-vs-string-literal source
widening).

Zero regressions.
