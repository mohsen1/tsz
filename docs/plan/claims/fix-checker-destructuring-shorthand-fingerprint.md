# [WIP] fix(checker): align shorthand destructuring diagnostics

- **Date**: 2026-04-28
- **Branch**: `fix/checker-destructuring-shorthand-fingerprint`
- **PR**: #1704
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance / fingerprint-only)

## Intent

Fix the fingerprint-only conformance mismatch in
`shorthandPropertyAssignmentsInDestructuring_ES6.ts`, where tsz reports the
same TS1312 and TS2322 codes as tsc but with divergent diagnostic fingerprints.
The slice will diagnose whether the mismatch is display, anchor, or elaboration
policy and route the fix through the appropriate checker/solver boundary.

## Files Touched

- `crates/tsz-checker/src/assignability/assignment_checker/destructuring.rs`
- `crates/tsz-checker/src/types/type_checking/core.rs`
- `crates/tsz-checker/src/types/computation/object_literal/computation.rs`
- `crates/tsz-parser/src/parser/node.rs`
- `crates/tsz-parser/src/parser/state_expressions_literals.rs`
- `crates/tsz-checker/tests/binding_pattern_inference_tests.rs`
- `crates/tsz-checker/tests/value_usage_tests.rs`

## Verification

- `cargo nextest run -p tsz-checker test_destructuring_default_literal_mismatch_reports_initializer_type test_invalid_shorthand_property_default_anchors_ts1312_on_equals` (2 passed)
- `./scripts/conformance/conformance.sh run --filter "shorthandPropertyAssignmentsInDestructuring_ES6" --verbose` (1/1 passed)
- `cargo check --package tsz-parser`, `cargo check --package tsz-checker`, and `cargo check --package tsz-solver` passed.
- `cargo nextest run --package tsz-checker --lib` currently fails on pre-existing/non-slice gates: LOC cap drift in unrelated files plus `test_number_literal_to_numeric_enum_type` and `test_ts2322_assignment_destructuring_defaults_report_undefined_mismatches`; this PR remains WIP until those are reconciled or scoped.
