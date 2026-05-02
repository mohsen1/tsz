# fix(jsx,solver): emit TS2698 for spread of `T extends any` and unknown-typed spreads

- **Date**: 2026-05-02
- **Branch**: `fix/spread-type-extends-any`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance — TS2698 false negatives in JSX class-component and constraint-chain spreads)

## Intent

`tsc` normalizes `T extends any` to `T extends unknown` in
`getConstraintFromTypeParameter` and rejects spreading the result with
TS2698. Two issues prevented tsz from matching this:

1. Solver `is_valid_spread_type` resolved a type parameter's constraint
   with a single non-recursive lookup and treated `any` as spreadable.
   Fix: walk the constraint chain (mirroring tsc's
   `getResolvedBaseConstraint`) and replace `any` with `unknown` along
   the way, so the existing `Intrinsic(Unknown)` arm rejects the spread.
2. JSX checker only emitted TS2698 from
   `check_jsx_attributes_against_props`, but the orchestration takes
   different paths for class components with multiple constructor
   overloads (`React.ComponentClass<P>` from `react16.d.ts`) and for
   components whose props extraction falls back. Fix: extracted the
   spread-validity walk into `check_jsx_spread_attrs_for_ts2698`, called
   from the JSX orchestration entry, so TS2698 fires once per spread
   regardless of the downstream path.

## Files Touched

- `crates/tsz-solver/src/type_queries/core.rs` (+31 / -2)
- `crates/tsz-solver/src/tests/type_queries_spread_tests.rs` (+116 new tests)
- `crates/tsz-checker/src/checkers/jsx/orchestration/resolution.rs` (+11)
- `crates/tsz-checker/src/checkers/jsx/overloads.rs` (+2)
- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs` (~32 modified)
- `crates/tsz-checker/src/checkers/jsx/props/validation.rs` (+52)
- `crates/tsz-checker/tests/jsx_component_attribute_tests.rs` (+76 new tests)
- `scripts/conformance/conformance-baseline.txt` (-1: `mappedTypeRecursiveInference.ts` shifts off the wrong-code list)

## Verification

- `cargo nextest run -p tsz-solver --lib` — 5589 tests pass (4 new spread tests).
- `cargo nextest run -p tsz-checker -E 'test(jsx_)'` — 322 JSX tests pass (3 new TS2698 tests).
- `./scripts/conformance/conformance.sh run --filter destructuringParameterDeclaration1ES6` — 1/1 pass (was wrong-code).
- `./scripts/conformance/conformance.sh run --filter jsxExcessPropsAndAssignability` — actual `[TS2698]`, expected `[TS2322,TS2698]` — TS2698 now emitted; remaining TS2322 (whole-element merged-spread assignability) is a separate feature gap, out of scope.
- `./scripts/conformance/conformance.sh run --filter mappedTypeRecursiveInference` — codes match (TS2345 ↔ TS2345); remaining gap is fingerprint-only (property iteration order in `Deep<XMLHttpRequest>`), unrelated to the spread fix and tracked under Workstream 1's fingerprint-parity bucket.

## Notes

The original commit message claimed `mappedTypeRecursiveInference.ts`
was "fully fixed". After rebasing onto main (`069f63fb35` —
`fix(checker): validate intrinsic JSX tag against JSX.ElementType on
success`), the test sits at fingerprint-only rather than wrong-code.
The error code now matches; only display order differs. The spread-side
improvements that drove the previous error-code shift remain valid.
