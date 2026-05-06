# fix(checker): realign jsxElementType fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/jsx-element-type-regression-20260506-183000`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

Target `TypeScript/tests/cases/compiler/jsxElementType.tsx`.
The current canonical picker reports a fingerprint-only mismatch with the
expected diagnostic code set still present (`TS2304`, `TS2322`, `TS2339`,
`TS2741`, `TS2769`, `TS2786`). PR #3200 previously fixed this fixture and was
merged on 2026-05-05, so this slice will identify the current regression and
realign the JSX element-type diagnostics without changing the diagnostic code
set.

## Files Touched

- TBD after investigation.

## Verification

- Focused Rust regression in the owning JSX checker path.
- `./scripts/conformance/conformance.sh run --filter "jsxElementType" --verbose`
