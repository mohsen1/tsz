# Fix JSX generic function attr error anchoring and intersection display

- **Date**: 2026-05-01
- **Branch**: `fix/jsx-generic-function-attr-error-anchoring`
- **PR**: TBD
- **Status**: claim
- **Workstream**: Conformance — JSX fingerprint parity

## Intent

Fix fingerprint mismatch in `checkJsxGenericTagHasCorrectInferences`. When a
function-valued JSX attribute produces a body-level type error, suppress the
body-level TS2322 and anchor the error at the attribute name. Display the target
type as the intersection of callable types using `&` notation, matching tsc.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/props/resolution.rs` (~40 LOC change)
- `crates/tsz-checker/src/types/utilities/core.rs` (~30 LOC new helper)
- `crates/tsz-checker/src/checkers/jsx/orchestration/component_props.rs` (~10 LOC)
- `crates/tsz-checker/tests/jsx_component_attribute_tests.rs` (~60 LOC new test)
- `crates/tsz-solver/src/diagnostics/format/compound.rs` (~10 LOC)
- `crates/tsz-solver/src/intern/type_factory.rs` (~8 LOC new method)

## Verification

- `checkJsxGenericTagHasCorrectInferences.tsx`: 1/1 passed
- `cargo nextest run -p tsz-checker -p tsz-solver`: 11355+ tests pass
- New unit test: `test_generic_jsx_function_attr_error_anchors_at_attribute_not_body`
