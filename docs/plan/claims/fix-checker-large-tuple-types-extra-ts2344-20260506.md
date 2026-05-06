# [STALE] fix(checker): remove largeTupleTypes extra TS2344

- **Date**: 2026-05-06
- **Branch**: `fix/large-tuple-types-extra-ts2344-20260506`
- **PR**: #3756
- **Status**: stale
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the quick-picked false-positive target
`TypeScript/tests/cases/compiler/largeTupleTypes.ts`.
The TypeScript baseline expects no diagnostics, while tsz currently reports an
extra `TS2344`, so this slice is scoped to root-causing and removing that
checker false positive with focused Rust coverage.

## Files Touched

- `docs/plan/claims/fix-checker-large-tuple-types-extra-ts2344-20260506.md`
  (claim)

## Verification

- `CARGO_BUILD_JOBS=2 ./scripts/conformance/conformance.sh run --filter "largeTupleTypes" --verbose` (1/1 passed on current `origin/main`)

## Notes

The quick-pick shortlist was stale for this target. Current `origin/main`
already matches the TypeScript baseline, so PR #3756 was closed without an
implementation change.
