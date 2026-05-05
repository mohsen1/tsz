# fix(checker): shadowed NaN identifiers trigger false TS2845

- **Date**: 2026-05-05
- **Branch**: `claude/ecstatic-faraday-waS0T`
- **PR**: #2950
- **Status**: ready
- **Workstream**: conformance / false-positive reduction

## Intent

`is_identifier_reference_to_global_nan` in `helpers.rs` accepted any
parentless symbol named `NaN` as the global `NaN`. Module-level user
declarations also have `parent.is_none()`, so `const NaN = 0;` at module
scope falsely triggered TS2845. Fix: rely only on `symbol_is_from_lib` to
identify the lib `NaN`; drop the incorrect `|| is_global` fallback.

## Files Touched

- `crates/tsz-checker/src/types/computation/helpers.rs` (~5 LOC change)
- `crates/tsz-checker/tests/conformance_issues/features/async.rs` (new tests)

## Verification

- `cargo nextest run -p tsz-checker`
- Targeted conformance filter for NaN-related tests
