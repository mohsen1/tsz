# fix(checker): align intrinsic JSX ref callback diagnostic fingerprint

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next3`
- **PR**: https://github.com/mohsen1/tsz/pull/2753
- **Status**: ready
- **Workstream**: 1 (Conformance fixes)

## Intent

This PR targets the fingerprint-only failure in
`TypeScript/tests/cases/conformance/jsx/tsxStatelessFunctionComponents2.tsx`.
Both `tsc` and `tsz` emit `TS2339`; the remaining gap is the exact diagnostic
fingerprint for the intrinsic `div` `ref` callback parameter access:

```tsx
let i = <div ref={x => x.propertyNotOnHtmlDivElement} />;
```

`tsc` reports that `propertyNotOnHtmlDivElement` does not exist on
`HTMLDivElement`. `tsz` emitted the same diagnostic code set but missed this
fingerprint because the JSX special `ref` branch contextually typed callback
parameters, then return-type inference rolled back expression-body diagnostics
without rechecking that body through the special-attribute path.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs`
  and `crates/tsz-checker/src/checkers/jsx/props/special_attribute_callbacks.rs`
  - Rechecks expression-bodied function-valued JSX `key`/`ref` attributes with
    the refined contextual callable type after the initial contextual pass.
  - Caches contextual parameter types before the recovery body check so
    intrinsic `ref` callback parameters keep their `HTMLDivElement` context.
  - Recovers missing-property diagnostics on contextual parameters while
    filtering declared/known DOM members that resolve to bare `any` in this
    recovery path.
- `crates/tsz-checker/src/checkers/jsx/props/synthesized_display.rs`
  - Moves the synthesized JSX prop display helper out of the touched
    over-limit implementation file to keep the checker boundary guardrail green.
- `crates/tsz-checker/src/checkers/jsx/ref_callback_tests.rs`
  - Extends the intrinsic `ref` callback coverage to assert the
    `HTMLDivElement` missing-property diagnostic and a declared-property
    no-false-positive case.
  - Adds inherited generic `HTMLProps<T> extends ... ClassAttributes<T>`
    coverage matching the React shape used by the conformance fixture.

## Verification

- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run -p tsz-checker --lib -E 'test(intrinsic_ref_callback_uses_html_element_context_without_false_positive) + test(intrinsic_ref_callback_uses_inherited_generic_html_props_context)'`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `./scripts/conformance/conformance.sh run --filter "tsxStatelessFunctionComponents2" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `./scripts/conformance/conformance.sh run`
  - `FINAL RESULTS: 12439/12582 passed (98.9%)`
  - Improvement includes
    `TypeScript/tests/cases/conformance/jsx/tsxStatelessFunctionComponents2.tsx`
