# [WIP] fix(checker): suppress TS2749 for namespace type-only export merge

- **Date**: 2026-04-29
- **Branch**: `fix/checker-namespace-type-only-export-ts2749`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

The quick-pick target is `TypeScript/tests/cases/compiler/namespacesWithTypeAliasOnlyExportsMerge.ts`, a false-positive `TS2749` where tsz reports a value-vs-type diagnostic that tsc does not. This PR will diagnose the namespace/type-only export merge path and fix the root cause in the checker, solver, or boundary layer that owns the invariant.

## Files Touched

- `docs/plan/claims/fix-checker-namespace-type-only-export-ts2749.md`
- Implementation files TBD after diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "namespacesWithTypeAliasOnlyExportsMerge" --verbose`
- Planned: owning-crate unit tests with `cargo nextest run`
