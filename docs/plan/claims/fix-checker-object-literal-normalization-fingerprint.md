# [WIP] fix(checker): align object literal normalization fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/checker-object-literal-normalization-fingerprint`
- **PR**: https://github.com/mohsen1/tsz/pull/2975
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the current fingerprint-only divergence in
`TypeScript/tests/cases/conformance/expressions/objectLiterals/objectLiteralNormalization.ts`.
The picker reports matching diagnostic code `TS2322`, so this PR will root-cause
the remaining diagnostic message, span, count, or ordering mismatch.

## Files Touched

- `docs/plan/claims/fix-checker-object-literal-normalization-fingerprint.md`
  (claim)

## Verification

- owning-crate regression test
- `./scripts/conformance/conformance.sh run --filter "objectLiteralNormalization" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
