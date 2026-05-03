# fix(checker): emit TS2609 on JSX spread of error-typed expressions

- **Date**: 2026-05-03
- **Branch**: `fix/jsx-spread-child-error-emits-ts2609`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — TS2609 chained-diagnostics for JSX spread of unresolved expressions)

## Intent

`normalize_jsx_spread_child_type` short-circuited at the top with
`matches!(spread_type, TypeId::ANY | TypeId::ERROR)` and silently returned
`TypeId::ANY` without emitting TS2609. tsc emits TS2609 ("JSX spread
child must be an array type") *alongside* upstream errors when the
spread expression's type fails to resolve — e.g.

```tsx
const MySFC = (props: { children?: JSX.Element[] }) =>
    <div>{...this.props.children}</div>;
//          ^^^^^^^^^^^^^^^^^^^^^^^ tsc: TS7041 + TS2609; tsz before: TS7041 only
```

The fix narrows the early-exit gate from `TypeId::ANY | TypeId::ERROR` to
just `TypeId::ANY`. Genuine `any` (explicit annotation, intentional
widening) still suppresses TS2609 (matching tsc's permissive behaviour
for `any` spreads); error-propagated `ERROR` types now flow into the
non-array fallback path that emits TS2609.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/children.rs` (+8 / -1) — narrow
  the early-exit gate, add a comment documenting the rule.
- `crates/tsz-checker/tests/jsx_component_attribute_tests.rs` (+62) —
  two new unit tests pinning structural rules across the gate change:
  `jsx_spread_child_void_value_emits_ts2609` (non-array, non-ANY,
  non-ERROR primitive must still emit) and
  `jsx_spread_child_explicit_any_value_still_no_ts2609` (genuine `any`
  must still suppress).

## Verification

- `cargo nextest run -p tsz-checker --test jsx_component_attribute_tests -E 'test(jsx_spread_child)'`
  — 5/5 spread-child tests pass (3 existing + 2 new).
- `cargo nextest run -p tsz-checker -E 'test(jsx)'` — 332 JSX tests pass.
- `./scripts/conformance/conformance.sh run --filter "inlineJsxFactoryDeclarationsLocalTypes" --verbose`
  — `inlineJsxFactoryDeclarationsLocalTypes.tsx` flips from
  fingerprint-only failure (1 missing TS2609) to **PASS (1/1)**.
- Full conformance: net **+2 vs plain main** (3 improvements, 4
  regressions vs plain main's 2 improvements / 5 regressions on the
  same `main` HEAD). My fix flips the target test to PASS *and*
  un-regresses `typeParameterConstModifiersReturnsAndYields.ts`. The 4
  declaration-emit regressions are pre-existing snapshot drift
  unrelated to this PR (they appear identically without my patch
  applied).

## Notes

The unit-test harness can't reproduce the exact `this`-resolves-to-ERROR
path (lib loading + strictNullChecks + noImplicitThis combination is
non-trivial to set up without the conformance harness's full lib
graph). The conformance test acts as the end-to-end lock; the unit
tests pin the structural rules that bracket the gate change.
