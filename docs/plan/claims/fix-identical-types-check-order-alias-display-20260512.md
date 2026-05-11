# fix(checker): preserve alias arguments in identical type check-order diagnostics

- **Date**: 2026-05-12
- **Branch**: `fix/identical-types-check-order-alias-display-20260512`
- **PR**: https://github.com/mohsen1/tsz/pull/5646
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Close the `identicalTypesNoDifferByCheckOrder.ts` fingerprint-only
conformance failure. The current diagnostic expands aliases such as
`SomePropsX` into `Required<Pick<...>> & Omit<...>` inside
`FunctionComponent<T>` source display; tsc preserves the local alias argument
name in the TS2322 message.

## Files Touched

- `crates/tsz-checker/src/error_reporter/assignability_alias_display.rs`
- `docs/plan/claims/fix-identical-types-check-order-alias-display-20260512.md`

## Verification

- Baseline: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter identicalTypesNoDifferByCheckOrder --verbose` (0/1, fingerprint-only)
- `cargo fmt --all --check` (passed)
- `cargo test -p tsz-checker --test conformance_issues test_variance_reference_assignability_uses_tsc_alias_display -- --nocapture` (passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter identicalTypesNoDifferByCheckOrder --verbose` (1/1 passed, fingerprint-only 0)
- `git diff --check` (passed)
