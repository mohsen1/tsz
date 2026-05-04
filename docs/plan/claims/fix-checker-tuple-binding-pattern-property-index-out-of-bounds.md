---
name: tuple-out-of-bounds for object-binding-pattern numeric property keys
status: claimed
timestamp: 2026-05-04 17:50:00
branch: fix/checker-tuple-binding-pattern-property-index-out-of-bounds
---

# Claim

Workstream 1 (Diagnostic Conformance) — TS2493 emission for object
binding patterns whose property name is a numeric literal indexing a
fixed-length tuple beyond its declared length.

## Problem

For `function c(...{0: a, length, 3: d}: [boolean, string, number]) { }`,
tsc emits:

```
TS2493: Tuple type '[boolean, string, number]' of length '3' has no element at index '3'.
```

at the `3:` property key. tsz emitted nothing for this site.

The element-access path (e.g. `tuple[3]`) already emits TS2493 via the
generic property/element-access route, but the destructuring path —
specifically `get_binding_element_type_with_request` for object binding
patterns — never bounds-checked numeric property keys against the
parent tuple's length. Property-name resolution succeeded silently and
returned the element type at the index (which falls through to
"property not found" handling but, for tuple types, doesn't emit
TS2493 the way array-binding-pattern bounds checks do).

## Fix

In `crates/tsz-checker/src/state/variable_checking/destructuring.rs`,
right after `extract_binding_property_name` resolves the static
property name, add a tuple-bounds short-circuit for fixed-length
tuples (those without a rest element):

```rust
if let Some(prop_name_str) = property_name.as_deref()
    && let Ok(idx) = prop_name_str.parse::<usize>()
    && let Some(elems) = query::tuple_elements(self.ctx.types, parent_type)
    && !elems.iter().any(|e| e.rest)
    && idx >= elems.len()
{
    // emit TS2493 with the standard message and return UNDEFINED
}
```

Rest-bearing tuples (e.g. `[T, ...T[]]`) accept any non-negative index
by design, so they are not bounds-checked here. The check returns
`TypeId::UNDEFINED` so nested binding patterns receive the same
out-of-bounds element type tsc uses, allowing follow-on diagnostics
(TS2488 etc.) to fire correctly.

## Files Touched

- `crates/tsz-checker/src/state/variable_checking/destructuring.rs`
  (+33 / 0) — add tuple-bounds short-circuit in
  `get_binding_element_type_with_request` for object binding patterns
  with numeric-literal property names.
- `crates/tsz-checker/tests/tuple_index_access_tests.rs` (+72 / 0) —
  three new structural tests:
  - `ts2493_object_binding_pattern_numeric_property_on_tuple_out_of_bounds`
  - `ts2493_object_binding_pattern_numeric_property_on_tuple_param_name_independent`
    (different binding/key names — locks the rule structurally per
    CLAUDE.md §25 anti-hardcoding directive)
  - `ts2493_object_binding_pattern_numeric_property_in_bounds_does_not_fire`
    (locks the inverse — in-bounds numeric keys do NOT fire)

## Tests

- New: 3 structural tests pass (all pinned).
- Crate suite: `cargo nextest run -p tsz-checker` — 6218 passed
  (6221 with the 3 new tests), 36 skipped.
- Targeted conformance: `restParameterWithBindingPattern3.ts` flips
  PASS, plus the existing `restParameterWithBindingPattern1.ts` and
  `...2.ts` continue to pass.

## Conformance impact

`12418 → 12421 (+3)`. Improvements:
- `restParameterWithBindingPattern3.ts` (the targeted fix)
- `excessPropertyCheckWithMultipleDiscriminants.ts`,
  `inlineJsxAndJsxFragPragma.tsx`,
  `labeledStatementDeclarationListInLoopNoCrash3.ts`
  (incidental wins from running on a fresh main).

The single listed regression
(`objectLiteralExcessProperties.ts`) is pre-existing drift on plain
main (verified by stash + rerun: identical TS2353 union-display
mismatch without the fix).
