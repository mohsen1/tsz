# [WIP] fix(checker): align excessively large tuple spread diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-excessively-large-tuple-spread-diagnostics`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the current wrong-code divergence in
`TypeScript/tests/cases/compiler/excessivelyLargeTupleSpread.ts`.
The picker reports expected diagnostics `TS2799` and `TS2800`, while tsz
currently emits `TS2589`, so this PR will root-cause tuple spread size handling
and align the emitted diagnostics.

## Files Touched

- `docs/plan/claims/fix-checker-excessively-large-tuple-spread-diagnostics.md`
  (claim)

## Verification

- owning-crate regression test
- `./scripts/conformance/conformance.sh run --filter "excessivelyLargeTupleSpread" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
