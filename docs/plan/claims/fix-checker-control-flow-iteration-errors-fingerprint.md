# [WIP] fix(checker): align control flow iteration errors fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/checker-control-flow-iteration-errors-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the current fingerprint-only divergence in
`TypeScript/tests/cases/conformance/controlFlow/controlFlowIterationErrors.ts`.
The picker reports matching diagnostic codes `TS2345` and `TS2769`, so this PR
will root-cause the remaining diagnostic message, span, count, or ordering
mismatch.

## Files Touched

- `docs/plan/claims/fix-checker-control-flow-iteration-errors-fingerprint.md`
  (claim)

## Verification

- owning-crate regression test
- `./scripts/conformance/conformance.sh run --filter "controlFlowIterationErrors" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
