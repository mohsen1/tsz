# fix(checker): expand recursive union alias in assignment diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-recursive-union-alias-display`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the remaining fingerprint-only divergence in
`TypeScript/tests/cases/compiler/unionTypeWithRecursiveSubtypeReduction3.ts`.
The previous `fix/checker-typeof-typeliteral-no-circular` slice removed the
false `TS2456`; the current gap is the `TS2322` display text.

`tsc` expands the recursive `typeof` alias enough to show
`{ prop: number; } | { prop: { prop: number; } | ...; }`, while `tsz`
prints the alias name `T27`. This slice will align the assignment diagnostic
display without reintroducing circularity errors.

## Files Touched

- TBD

## Verification

- Baseline targeted conformance:
  - `./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "unionTypeWithRecursiveSubtypeReduction3" --verbose`
  - Current result: fingerprint-only `TS2322`; expected display expands the
    recursive union and actual display is `T27`.
