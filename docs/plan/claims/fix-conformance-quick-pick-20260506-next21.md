# [WIP] fix(checker): suppress tuple spread missing-property cascade

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-quick-pick-20260506-next21`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Claiming `TypeScript/tests/cases/conformance/expressions/arrayLiterals/arrayLiterals3.ts`.

Current `origin/main` emits the expected TS2322 for the file, but also emits an
extra TS2739 at `var c0: tup = [...temp2]`. The slice will align array-literal
spread assignment diagnostics with tsc by suppressing the missing-property
cascade without weakening ordinary TS2739 object-shape diagnostics.

## Files Touched

- TBD after root-cause investigation.

## Verification

- Pending.
