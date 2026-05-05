# fix(checker): align jsxElementType diagnostic fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-jsx-element-type-fingerprints`
- **PR**: #3055
- **Status**: claim
- **Workstream**: 1 (Conformance - JSX diagnostic fingerprints)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/compiler/jsxElementType.tsx`, currently a
fingerprint-only failure: `tsz` emits the same diagnostic code set as `tsc`
(`TS2304`, `TS2322`, `TS2339`, `TS2741`, `TS2769`, `TS2786`) but differs in
one or more diagnostic fingerprints.

This PR will inspect the remaining message/anchor/display divergence, fix the
root cause in the appropriate checker/solver/printer layer, and add a focused
Rust regression test for the invariant.

## Files Touched

- TBD after implementation.

## Verification

- `./scripts/conformance/conformance.sh run --filter "jsxElementType" --verbose`
- focused Rust regression test in the owning crate
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
