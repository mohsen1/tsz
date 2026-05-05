# fix(checker): align discriminated union types2 fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-discriminated-union-types2-fingerprints`
- **PR**: https://github.com/mohsen1/tsz/pull/2953
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the current fingerprint-only divergence in
`TypeScript/tests/cases/conformance/types/union/discriminatedUnionTypes2.ts`.
The picker reports matching diagnostic codes (`TS2339`, `TS2353`), so this PR
will root-cause the remaining diagnostic message, span, count, or ordering
mismatch. The older claim for PR #1797 is stale and already merged.

## Files Touched

- `docs/plan/claims/fix-checker-discriminated-union-types2-fingerprints.md`
  (claim)

## Verification

- owning-crate regression test
- `./scripts/conformance/conformance.sh run --filter "discriminatedUnionTypes2" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
