# [WIP] fix(checker): align booleanFilterAnyArray conformance

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-boolean-filter-any-array-20260506`
- **PR**: #3686
- **Status**: stale
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the quick-pick conformance target
`TypeScript/tests/cases/compiler/booleanFilterAnyArray.ts`. The saved snapshot
shows a one-diagnostic drift with an extra `TS2403`; this slice is scoped to
root-causing that false positive and landing the fix in the owning checker path
with focused Rust coverage.

## Files Touched

- `docs/plan/claims/fix-conformance-boolean-filter-any-array-20260506.md`

## Verification

- `CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --filter "booleanFilterAnyArray" --verbose` (1/1 passed on current `origin/main`; saved snapshot claim is stale)
