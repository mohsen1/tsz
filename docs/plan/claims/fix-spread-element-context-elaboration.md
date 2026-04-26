**2026-04-26 16:05:00**

# fix(checker): elaborate array-literal spread element mismatch to spread expression

- **Date**: 2026-04-26
- **Branch**: `fix/spread-element-context-elaboration`
- **PR**: #1413
- **Status**: ready
- **Workstream**: 1 (conformance)

## Intent

When an array literal contains a spread element whose iterated element type
is not assignable to the contextual array element type (e.g.
`var array: number[] = [0, 1, ...new SymbolIterator]`), tsc reports
`TS2322 'symbol' is not assignable to 'number'` at the spread expression
position. tsz previously widened the array literal to `(number | symbol)[]`
and emitted the assignment error at the variable position.

This PR teaches `try_elaborate_array_literal_elements` to drill into spread
elements: when the contextual target is a plain `T[]` / `readonly T[]` and
the spread's `for_of_element_type` is not assignable to the contextual
element type, emit TS2322 anchored on the spread element node.

Restricted to plain array targets so custom interfaces extending `Array<T>`
keep tsc's whole-assignment TS2322 message and position.

Format types via `format_type` rather than the assignability diagnostic
pipeline (which would re-derive the source display from the spread
receiver, e.g. `'SymbolIterator'`, instead of the iterated element type
`'symbol'`).

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs` (~125 LOC: spread-elaboration helper + plain-array gate)
- `crates/tsz-checker/tests/spread_rest_tests.rs` (~85 LOC: positive + negative regression tests)
- `docs/plan/claims/fix-spread-element-context-elaboration.md` (this file)

## Verification

- Targeted: `./scripts/conformance/conformance.sh run --filter "iteratorSpreadInArray5"` passes (1/1)
- Adjacent: `./scripts/conformance/conformance.sh run --filter "iteratorSpread"` (23/23), `--filter "arrayLiteral"` (19/19)
- Unit: `cargo nextest run -p tsz-checker --lib` (2890/2890)
- Unit: `cargo nextest run -p tsz-checker --test spread_rest_tests` (65/65)
- Architecture: contract test passes (no direct `TypeData::ReadonlyType` import — uses `query_boundaries::common::get_readonly_inner`)
