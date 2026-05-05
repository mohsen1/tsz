# fix(checker): align object literal excess property fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-object-literal-excess-property-fingerprints`
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/compiler/objectLiteralExcessProperties.ts`.
The picked test already emits the same diagnostic codes as `tsc`
(`TS2304`, `TS2322`, `TS2353`, `TS2561`), so this PR aligns the
TS2353 target display for an excess property on the concrete side of a
generic union.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/property.rs`
- `crates/tsz-checker/tests/ts2353_tests.rs`
- `docs/plan/claims/fix-checker-object-literal-excess-property-fingerprints.md`

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-checker`
- `cargo test -p tsz-checker --test ts2353_tests -- --nocapture`
  - `test result: ok. 36 passed; 0 failed`
- `cargo test --package tsz-checker --lib`
  - `test result: ok. 3359 passed; 0 failed; 10 ignored`
- `./scripts/conformance/conformance.sh run --filter "objectLiteralExcessProperties" --verbose`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
  - `Fingerprint-only: 0`
- `./scripts/conformance/conformance.sh run --max 200`
  - `FINAL RESULTS: 200/200 passed (100.0%)`
  - `Fingerprint-only: 0`
