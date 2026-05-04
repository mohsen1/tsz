# [WIP] fix(parser): align JSX invalid Esprima fingerprint

- **Date**: 2026-05-04
- **Branch**: `fix/parser-jsx-invalid-esprima-fingerprint`
- **PR**: #2714
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Investigate and fix the fingerprint-only diagnostic mismatch in
`TypeScript/tests/cases/conformance/jsx/jsxInvalidEsprimaTestSuite.tsx`.
The codes already match TypeScript, so this slice is scoped to parser recovery
diagnostic positions/messages for invalid JSX syntax.

## Files Touched

- `docs/plan/claims/fix-parser-jsx-invalid-esprima-fingerprint.md`
- Implementation files TBD after localizing the parser recovery mismatch.

## Verification

- Reproduced: `./scripts/conformance/conformance.sh run --filter "jsxInvalidEsprimaTestSuite" --verbose`
- Planned: targeted parser/checker tests for the owning parser recovery path.
