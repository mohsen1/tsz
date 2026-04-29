# [WIP] fix(checker): align signature group identity diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/signature-group-identity-fingerprint`
- **PR**: #1715
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Investigate the fingerprint-only conformance failure in
`orderMattersForSignatureGroupIdentity.ts`. The PR will align TSZ's diagnostic
fingerprints with `tsc` for the signature group identity case, fixing the
owning checker/solver/printer invariant rather than adding a local suppression.

## Files Touched

- `docs/plan/claims/fix-signature-group-identity-fingerprint.md`
- `crates/tsz-checker/src/types/computation/call_result.rs`
- `crates/tsz-checker/tests/conformance_issues/errors/callable_objects.rs`

## Verification

- Passed: `./scripts/conformance/conformance.sh run --filter "orderMattersForSignatureGroupIdentity" --verbose`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
- Passed: `cargo nextest run --package tsz-checker test_failed_overload_call_returns_never_for_follow_on_property_access`
- Passed: `cargo nextest run --package tsz-checker generic_object_assign_initializer_keeps_outer_ts2322`
- Passed: `cargo nextest run --package tsz-checker --lib`
  - `2960 tests passed, 11 skipped`
- Passed with existing baseline miss: `./scripts/conformance/conformance.sh run --max 200`
  - `FINAL RESULTS: 199/200 passed (99.5%)`
  - Reported `No regressions or improvements vs baseline`
- Passed: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
  - `FINAL RESULTS: 12236/12582 passed (97.3%)`
