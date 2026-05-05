# [WIP] fix(checker): align mapped type errors current fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-mapped-type-errors-current-fingerprints`
- **PR**: https://github.com/mohsen1/tsz/pull/2971
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the current fingerprint-only divergence in
`TypeScript/tests/cases/conformance/types/mapped/mappedTypeErrors.ts`.
The picker reports matching diagnostic codes (`TS2313`, `TS2322`, `TS2339`,
`TS2344`, `TS2345`, `TS2353`, `TS2403`, `TS2536`), so this PR will
root-cause the remaining diagnostic message, span, count, or ordering mismatch.

Older mapped-type-error claims and PRs already merged; this claim tracks the
fresh drift selected from current `origin/main`.

## Files Touched

- `docs/plan/claims/fix-checker-mapped-type-errors-current-fingerprints.md`
  (claim)

## Verification

- owning-crate regression test
- `./scripts/conformance/conformance.sh run --filter "mappedTypeErrors" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
