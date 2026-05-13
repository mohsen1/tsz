# fix(checker): suppress false TS2786 for complex JSX signatures

- **Date**: 2026-05-13
- **Branch**: `fix/conformance-ts2786-complex-signatures-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance

## Intent

Remove the final conformance blocker on current `main`: `callsOnComplexSignatures.tsx` emits one extra TS2786. The fix should address JSX component validation for the relevant complex signature shape without weakening real invalid-component diagnostics.

## Files Touched

- `docs/plan/claims/fix-conformance-ts2786-complex-signatures-20260513.md`

## Verification

- Pending
