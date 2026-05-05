# [WIP] fix(checker): preserve failed Object.assign fallback assignment diagnostics

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

This PR targets the fingerprint-only failure in
`TypeScript/tests/cases/conformance/types/typeRelationships/typeInference/unionAndIntersectionInference1.ts`.
`tsc` reports both the inner `TS2769` for an invalid generic `Object.assign`
call and the outer `TS2322` when the failed call's fallback return type is
assigned to an explicitly typed variable. `tsz` currently emits the overload
failure but misses the outer assignment diagnostic.

## Files Touched

- `crates/tsz-checker/src/types/computation/` (expected call-result/fallback investigation)
- `crates/tsz-checker/src/state/variable_checking/` or assignability boundary if the missing diagnostic is suppressed there
- `crates/tsz-checker/tests/` or an owning checker test module for the regression

## Verification

- Pending implementation.
