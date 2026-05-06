# fix(checker): realign JSX children property fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/jsx-children-property4-regression-20260506-170500`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

Target `TypeScript/tests/cases/conformance/jsx/checkJsxChildrenProperty4.tsx`.
The current picker reports a fingerprint-only mismatch with the expected
diagnostic codes (`TS2322`, `TS2551`) still present. PR #2812 previously fixed
this fixture, so this slice will identify the current drift or regression and
align the JSX children diagnostics without changing the diagnostic code set.

## Files Touched

- TBD after investigation.

## Verification

- Focused Rust regression in the owning JSX checker path.
- `./scripts/conformance/conformance.sh run --filter "checkJsxChildrenProperty4" --verbose`
