# [WIP] fix(checker): align TS2351 fingerprint for ESM module exports

- **Date**: 2026-04-29
- **Branch**: `fix/checker-esm-module-exports-ts2351-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Investigate and fix the quick-pick fingerprint-only mismatch in
`esmModuleExports2.ts`. The current divergence is a missing expected TS2351
fingerprint at `importer-cts.cts:5:5` for constructing a CommonJS import of an
ESM module with a `"module.exports"` export; diagnostic codes already match, so
the expected scope is message, anchor, or diagnostic formatting parity.

## Files Touched

- `docs/plan/claims/fix-checker-esm-module-exports-ts2351-fingerprint.md`
- Implementation files TBD after root-cause investigation.

## Verification

- `./scripts/conformance/conformance.sh run --filter "esmModuleExports2" --verbose` (baseline: fingerprint-only failure)
