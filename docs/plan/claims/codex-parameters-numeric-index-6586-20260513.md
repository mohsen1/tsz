# fix(solver): allow numeric index on Parameters<T>

- **Date**: 2026-05-13
- **Branch**: `codex/parameters-numeric-index-6586-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance / solver false positives

## Intent

Fix #6586 so indexed access like `Parameters<T>[0]` works when `T` is a
generic function type parameter constrained to a callable. This should remove
the false TS2536 while preserving real invalid indexed-access diagnostics.

## Files Touched

- `crates/tsz-solver/src/*` (expected)
- `crates/tsz-checker/tests/*` (expected)
- `docs/plan/claims/codex-parameters-numeric-index-6586-20260513.md`

## Verification

- Pending.
