# fix(checker): align conflicting inference literal display

- **Date**: 2026-05-05
- **Branch**: `fix/conflicting-inference-literal-display-20260505`
- **PR**: https://github.com/mohsen1/tsz/pull/2775
- **Status**: ready
- **Workstream**: conformance fingerprint parity

## Intent

Fix the remaining fingerprint-only divergence in
`TypeScript/tests/cases/compiler/typeInferenceConflictingCandidates.ts`.
Prior merged work made `tsz` emit the expected `TS2345`; this follow-up targets
the documented literal display mismatch where the diagnostic source/target text
uses widened primitive types instead of the literal forms that `tsc` prints.

## Files Touched

- `crates/tsz-checker/src/types/computation/call/inner.rs`
- `crates/tsz-checker/src/types/computation/call_inference.rs`
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`

## Root Cause

The checker already detected direct primitive conflicts for bare generic
parameters during round-1 inference and recorded the first literal candidate.
That record was only used while contextualizing round-2 argument collection. The
final generic solve still exposed the solver's widened instantiated parameter
(`string`), so TS2345 rendered `number` vs `string` instead of the first
candidate literal `3` vs `""`.

The fix reapplies that recorded conflict substitution to final instantiated
bare-`T` parameter slots when the literal is a subtype of the solver's widened
parameter. The owning regression locks the context-sensitive callback case:
`g("", 3, a => a)` now reports `Argument of type '3' is not assignable to
parameter of type '""'.`

## Verification

- `cargo fmt --all`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run -p tsz-checker direct_generic_argument_mismatch_survives_context_sensitive_callback`
- `cargo nextest run --package tsz-checker --lib`
- `cargo nextest run --package tsz-solver --lib`
- `./scripts/conformance/conformance.sh run --filter "typeInferenceConflictingCandidates" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
  - `FINAL RESULTS: 12444/12582 passed (98.9%)`
