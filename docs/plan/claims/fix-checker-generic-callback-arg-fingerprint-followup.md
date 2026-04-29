# [WIP] fix(checker): align generic callback TS2345 fingerprint

- **Date**: 2026-04-29
- **Branch**: `fix/checker-generic-callback-arg-fingerprint-followup`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only TS2345 mismatch in
`genericCallbackInvokedInsideItsContainingFunction1.ts`. The targeted
conformance run reports matching `TS2345`/`TS2558` codes, but tsz currently
misses the expected `Argument of type 'U' is not assignable to parameter of
type 'T'.` fingerprint for the `f(y)` call.

## Files Touched

- TBD after implementation.

## Verification

- Planned: `cargo check --package tsz-checker`
- Planned: `cargo check --package tsz-solver`
- Planned: `cargo build --profile dist-fast --bin tsz`
- Planned: owning crate unit tests with `cargo nextest run`
- Planned: `./scripts/conformance/conformance.sh run --filter "genericCallbackInvokedInsideItsContainingFunction1" --verbose`
- Planned: `./scripts/conformance/conformance.sh run --max 200`
