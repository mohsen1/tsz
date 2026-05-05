# fix(parser): align type guard function error fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/parser-type-guard-function-errors-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/conformance/expressions/typeGuards/typeGuardFunctionErrors.ts`.
The picker reports matching diagnostic codes (`TS1005`, `TS1128`, `TS1131`,
`TS1144`, `TS1434`), so this PR will root-cause the remaining parser
diagnostic message, span, count, or ordering mismatch.

## Files Touched

- `docs/plan/claims/fix-parser-type-guard-function-errors-fingerprint.md`
  (claim)

## Verification

- owning-crate regression test
- `./scripts/conformance/conformance.sh run --filter "typeGuardFunctionErrors" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
