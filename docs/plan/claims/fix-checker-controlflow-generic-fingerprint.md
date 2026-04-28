# fix(checker): align controlFlowGenericTypes diagnostic fingerprints

- **Date**: 2026-04-27
- **Branch**: `fix/checker-controlflow-generic-fingerprint`
- **PR**: #1609
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Fix the fingerprint-only conformance failure in
`controlFlowGenericTypes.ts`, where tsz emits the same diagnostic codes as
tsc but differs in message text or anchoring. The change will follow the
existing checker/solver diagnostic boundaries and add a focused regression
test for the narrowed behavior.

## Files Touched

- `crates/tsz-checker/src/types/property_access_type/resolve.rs`
- `crates/tsz-checker/src/assignability/assignability_diagnostics.rs`
- `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`
- `crates/tsz-checker/tests/conditional_keyof_test.rs`

## Verification

- `cargo test -p tsz-checker conditional_alias_assignable_to_partial_of_itself_has_no_ts2345`
- `cargo test -p tsz-checker generic_receiver_property_miss_reports_constraint_union_ts2339`
- `./scripts/conformance/conformance.sh run --filter "controlFlowGenericTypes" --verbose` (1/1 passed)
