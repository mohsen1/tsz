# fix(checker): report NaN equality comparisons

- **Date**: 2026-05-06
- **Branch**: `codex/nan-equality-ts2845-20260506`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance)

## Intent

Fix the `TypeScript/tests/cases/compiler/nanEquality.ts` conformance miss.
The current detail snapshot expects `TS2845` but tsz emits no diagnostic for
comparisons such as `x === NaN`.

The expected impact is a one-test conformance pass-rate increase by adding the
missing NaN comparison diagnostic while preserving shadowed local `NaN`
bindings.

## Files Touched

- `docs/plan/claims/codex-nan-equality-ts2845-20260506.md`

## Verification

- Pending implementation.
