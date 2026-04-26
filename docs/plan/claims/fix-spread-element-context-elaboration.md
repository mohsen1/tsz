# fix(checker): elaborate array-literal spread element mismatch to spread expression

- **Date**: 2026-04-26
- **Branch**: `fix/spread-element-context-elaboration`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (conformance)

## Intent

When an array literal contains a spread element whose iterated element type
is not assignable to the contextual array element type (e.g.
`var array: number[] = [0, 1, ...new SymbolIterator]`), tsc reports
`TS2322 'symbol' is not assignable to 'number'` at the spread expression
position. tsz currently widens the array literal to `(number | symbol)[]`
and emits the assignment error at the variable position.

This PR teaches `try_elaborate_array_literal_elements` to drill into spread
elements: when an array contextual element type is available and the
spread's `for_of_element_type` is not assignable to the contextual element
type, emit TS2322 at the spread expression with the iterated element type
vs. contextual element type.

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs` (~30 LOC)
- `crates/tsz-checker/src/error_reporter/call_errors/elaboration_array_mismatch.rs` (potentially)

## Verification

- `cargo nextest run -p tsz-checker --lib` (full pass)
- `./scripts/conformance/conformance.sh run --filter "iteratorSpreadInArray5"` (passes)
