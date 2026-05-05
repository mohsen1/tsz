# fix(checker): preserve generic callback literal target fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-generic-call-function-argument-fingerprints`
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/genericCallWithFunctionTypedArguments.ts`.
The picked test already emits the same diagnostic code as `tsc`
(`TS2345`), so this PR aligns callback target display when a later
literal argument fixes a generic return type.

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/display_formatting.rs`
- `crates/tsz-checker/tests/generic_inference_ordering_tests.rs`
- `docs/plan/claims/fix-checker-generic-call-function-argument-fingerprints.md`

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-checker`
- `cargo test -p tsz-checker --test generic_inference_ordering_tests -- --nocapture`
  - `test result: ok. 2 passed; 0 failed`
- `cargo test --package tsz-checker --lib`
  - `test result: ok. 3362 passed; 0 failed; 10 ignored`
- `./scripts/conformance/conformance.sh run --filter "genericCallWithFunctionTypedArguments" --verbose`
  - `FINAL RESULTS: 5/5 passed (100.0%)`
  - `Fingerprint-only: 0`
- `./scripts/conformance/conformance.sh run --max 200`
  - `FINAL RESULTS: 200/200 passed (100.0%)`
  - `Fingerprint-only: 0`
