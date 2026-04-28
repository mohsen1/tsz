# [WIP] fix(checker): align strict optional property fingerprints

- **Date**: 2026-04-28
- **Branch**: `fix/checker-strict-optional-properties-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

`scripts/session/quick-pick.sh` selected `TypeScript/tests/cases/compiler/strictOptionalProperties1.ts`, a fingerprint-only conformance failure. This PR will diagnose and align the remaining diagnostic fingerprint differences for exact optional property diagnostics while preserving the existing error-code set.

## Files Touched

- TBD after diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "strictOptionalProperties1" --verbose`
- Planned: owning-crate `cargo nextest run` coverage for the changed invariant.
