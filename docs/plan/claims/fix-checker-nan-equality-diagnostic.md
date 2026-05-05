# [WIP] fix(checker): report NaN equality diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-nan-equality-diagnostic`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the all-missing conformance failure in
`TypeScript/tests/cases/compiler/nanEquality.ts`. TypeScript reports `TS2845`
for equality comparisons against the global `NaN`, while tsz currently emits no
diagnostic for this target.

This is distinct from the earlier shadowed-`NaN` false-positive fix: local
parameters named `NaN` must remain accepted, but comparisons involving the
global lib `NaN` should be diagnosed.

## Files Touched

- `docs/plan/claims/fix-checker-nan-equality-diagnostic.md`

## Verification

- Pending implementation.
