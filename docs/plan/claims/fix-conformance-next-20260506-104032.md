# fix(parser): align rest parameter modifier diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/conformance-next-20260506-104032`
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Target the quick-pick conformance failure:
`TypeScript/tests/cases/compiler/restParamModifier.ts`.

`tsc` reports `TS1005` for the invalid constructor rest parameter modifier.
tsz currently also emits an extra `TS1213` strict-mode reserved-word/modifier
diagnostic on the recovered rest parameter. This slice will find the parser or
checker recovery path that emits the follow-up and suppress only the duplicate
diagnostic after the syntax error is already reported.

## Files Touched

- TBD

## Verification

- TBD
