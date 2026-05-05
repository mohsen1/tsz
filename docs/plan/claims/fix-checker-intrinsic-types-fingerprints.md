# fix(checker): preserve string intrinsic variance fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-intrinsic-types-fingerprints`
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/conformance/types/typeAliases/intrinsicTypes.ts`.
The picked test already emits the same diagnostic codes as `tsc`
(`TS2322`, `TS2344`, `TS2795`), so this PR aligns the missing
`Uppercase<T>` to `Uppercase<U>` assignability diagnostic.

## Files Touched

- `crates/tsz-checker/src/assignability/assignability_checker.rs`
- `crates/tsz-checker/src/assignability/assignment_checker_tests.rs`
- `docs/plan/claims/fix-checker-intrinsic-types-fingerprints.md`

## Verification

- `cargo check --package tsz-checker`
- `cargo test -p tsz-checker string_intrinsic_type_parameter_variance_emits_ts2322 --lib -- --nocapture`
- `cargo test --package tsz-checker --lib`
  - `test result: ok. 3351 passed; 0 failed; 10 ignored`
- `./scripts/conformance/conformance.sh run --filter "intrinsicTypes" --verbose`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
  - `Fingerprint-only: 0`
- `./scripts/conformance/conformance.sh run --max 200`
  - `FINAL RESULTS: 200/200 passed (100.0%)`
  - `Fingerprint-only: 0`
