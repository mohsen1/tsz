# [WIP] fix(parser): align type guard function error fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/parser-typeguard-function-errors-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the picked `typeGuardFunctionErrors.ts` fingerprint-only conformance
mismatch. The expected and actual diagnostic code sets already match
TypeScript (`TS1005`, `TS1128`, `TS1131`, `TS1144`, `TS1434`), so this slice
is scoped to parser recovery and diagnostic anchoring for malformed type guard
function declarations.

`scripts/session/quick-pick.sh --run` was attempted after the previous merge
but the local `dist-fast` build was killed silently while compiling
`tsz-checker`; the same picker without `--run` selected this case from the
available detail snapshot.

## Files Touched

- TBD

## Verification

- Pending
