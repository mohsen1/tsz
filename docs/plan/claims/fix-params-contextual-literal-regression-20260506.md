# fix(checker): realign params contextual literal diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/params-contextual-literal-regression-20260506-185439`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance fixes)

## Intent

Target `TypeScript/tests/cases/compiler/paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized.ts`.
The current conformance run reports a fingerprint-only TS2345 mismatch. PR
#2762 previously aligned this fixture, but tsz now displays the unresolved
mapped parameter target `{ [x in K]?: Lower<T>[] | undefined; }` for the first
argument, while tsc displays the instantiated key-specific targets
`{ y?: number[] | undefined; }` and `{ x?: string[] | undefined; }`.

## Files Touched

- TBD after investigation.

## Verification

- Focused Rust regression in the owning generic call inference/checker path.
- `./scripts/conformance/conformance.sh run --filter "paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized" --verbose`
