# [WIP] fix(checker): restore JSX excess props assignability diagnostic

- **Date**: 2026-05-05
- **Branch**: `fix/jsx-excess-props-assignability`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the conformance divergence in `jsxExcessPropsAndAssignability.tsx`.
The picker selected an only-missing failure where `tsc` emits `TS2322` and
`TS2698`, while `tsz` currently emits only `TS2698`.

## Files Touched

- TBD after root-cause investigation.

## Verification

- `./scripts/conformance/conformance.sh run --filter "jsxExcessPropsAndAssignability" --verbose`
- Focused Rust regression tests in the owning crate.
- Quick regression conformance sample before marking ready.
- Full pre-commit hook before push/ready state.
