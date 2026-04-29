# [WIP] fix(checker): align generic callback TS2345 fingerprint

- **Date**: 2026-04-29
- **Branch**: `fix/checker-generic-callback-arg-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Investigate and fix the fingerprint-only TS2345 mismatch in
`genericCallbackInvokedInsideItsContainingFunction1.ts`. The targeted
conformance run reports matching `TS2345`/`TS2558` codes, but the expected
`Argument of type 'U' is not assignable to parameter of type 'T'.` fingerprint
for the `f(y)` call is missing from tsz output.

## Files Touched

- TBD after root-cause investigation.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "genericCallbackInvokedInsideItsContainingFunction1" --verbose`
- Planned: owning crate unit tests with `cargo nextest run`
- Planned: quick conformance regression check
