# [WIP] fix(parser): align parameter syntax error fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/parameters-syntax-error-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/compiler/parametersSyntaxErrorNoCrash1.ts`, where
`tsc` and `tsz` both emit `TS1005` but the diagnostic fingerprint differs for
a malformed parameter type annotation.

## Files Touched

- `docs/plan/claims/fix-parameters-syntax-error-fingerprint.md`

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "parametersSyntaxErrorNoCrash1" --verbose`
- Focused Rust regression test in the owning crate
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
