# fix(checker): align exact optional inference TS2345 fingerprint

- **Date**: 2026-04-29
- **Branch**: `fix/checker-exact-optional-inference-fingerprint`
- **PR**: #1752
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Claim the random conformance target `TypeScript/tests/cases/compiler/inferenceExactOptionalProperties2.ts`.
The mismatch was a callback-body elaboration gap: after generic inference resolved
the callback parameter to `(actor: "counter") => void`, TSZ still emitted TS2345
on the whole callback argument instead of preserving the inner `spawn("alarm")`
diagnostic that tsc reports.

## Files Touched

- `docs/plan/claims/fix-checker-exact-optional-inference-fingerprint.md`
- `crates/tsz-checker/src/assignability/assignability_diagnostics.rs`
- `crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs`
- `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`
- `crates/tsz-checker/src/error_reporter/call_errors_tests.rs`

## Verification

- `cargo check --package tsz-solver`
- `cargo check --package tsz-checker`
- `cargo nextest run -p tsz-checker --lib -E 'test(contextual_generic_callback_preserves_inner_call_argument_mismatch)'`
- `cargo nextest run --package tsz-checker --lib`
- `./scripts/conformance/conformance.sh run --filter "inferenceExactOptionalProperties2" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
  - `FINAL RESULTS: 12241/12582 passed (97.3%)`
