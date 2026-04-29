# [WIP] fix(checker): align template literal pattern fingerprints

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429-5`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Investigate and fix the fingerprint-only mismatch in
`templateLiteralTypesPatterns.ts`, where TSZ reports the expected TS2322 and
TS2345 codes but differs from `tsc` in diagnostic fingerprint details. The fix
will preserve the shared checker/solver diagnostic boundaries and add a focused
unit test in the crate that owns the invariant.

## Files Touched

- TBD after investigation

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "templateLiteralTypesPatterns" --verbose`
- Planned: targeted `cargo nextest run` for changed crate(s)
