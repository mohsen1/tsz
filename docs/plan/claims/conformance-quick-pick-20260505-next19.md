# [WIP] fix(checker): align JSX overload diagnostic anchor

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next19`
- **PR**: #3262
- **Status**: claimed
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the quick-picked fingerprint-only mismatch in
`tsxStatelessFunctionComponentOverload4.tsx`. The diagnostic code set already
matches tsc (`TS2769`), but the overload diagnostic fingerprint differs for the
`TestingOptional` declaration surface around line 38.

## Files Touched

- TBD

## Verification

- `scripts/session/quick-pick.sh --run` selected and reproduced
  `TypeScript/tests/cases/conformance/jsx/tsxStatelessFunctionComponentOverload4.tsx`.
