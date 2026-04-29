# fix(checker): JSX overload TS2769 anchor for `b4` style — disagreeing-overload literal-attr

- **Date**: 2026-04-29
- **Branch**: `fix/checker-jsx-overload-anchor-literal-attr`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance and Fingerprints)

## Intent

Continuation of PR #1697 deferred scope. The c1/d1 source-type display
was fixed there; the b4 TS2769 anchor mismatch remains the last
fingerprint-only failure for
`contextuallyTypedStringLiteralsInJsxAttributes02.tsx`:

```ts
class MainButton {
    static (buttonProps: ButtonProps): JSX.Element;  // overload 1
    static (linkProps: LinkProps): JSX.Element;       // overload 2
}
const b4 = <MainButton goTo="home" extra />;          // line 34
```

- tsc:  `file.tsx:34:13` (anchored at `MainButton` — JSX tag name)
- tsz:  `file.tsx:34:24` (anchored at `goTo` attribute name)

## Root Cause (from iter 1 analysis)

`crates/tsz-checker/src/checkers/jsx/overloads.rs::jsx_overload_explicit_failure_attr`
walks each overload's failure attributes. For b4:

- Overload 1 (`ButtonProps`): no `goTo` → returns `Some("goTo")`
- Overload 2 (`LinkProps`): `goTo: "home" | "contact"`. tsz collects
  `attr.type_id` for `goTo="home"` once via `compute_type_of_node`
  *without* per-overload contextual typing, so the type is widened
  string. `is_assignable_to(string, "home"|"contact")` fails →
  `jsx_overload_explicit_failure_attr` returns `Some("goTo")`.

Both overloads return the same anchor → `all_overload_failures_share_explicit_anchor = true`
→ anchor at the `goTo` attribute name. tsc instead would see overload
2 succeed on `goTo` (literal `"home"` is contextually typed and
assignable) and fail on excess `extra`, so the two overloads return
different anchor names → fall through to tag-name anchoring.

## Fix Approaches

1. **Preserve literal types for syntactic JSX-attribute literal values**
   (`collect_jsx_provided_attrs`, line 302). Prefer
   `literal_type_from_initializer(value_idx)` over `compute_type_of_node`
   for `StringLiteral` / `NumericLiteral` / `True/FalseKeyword` values.
   This is the simplest fix — for our case, `goTo="home"` would have
   `attr.type_id = "home"` (literal), which IS assignable to
   `"home"|"contact"`, so overload 2's failure attr becomes `extra`.
2. **Per-overload contextual typing** — match tsc's `checkJsxAttribute`
   path which contextually types each value against the overload's
   prop type. This is the architecturally correct fix but a larger
   refactor.

Approach 1 is recommended. Verification needs to ensure no regressions
in existing JSX tests (`tsxStateless*`, `jsx_component_attribute_tests`).

## Verification

- New unit test in `crates/tsz-checker/tests/jsx_overload_anchor_literal_attr_tests.rs`
- `./scripts/conformance/conformance.sh run --filter "contextuallyTypedStringLiteralsInJsxAttributes02"`
  → expect flip to PASS
- 153/153 jsx_component_attribute_tests still pass
