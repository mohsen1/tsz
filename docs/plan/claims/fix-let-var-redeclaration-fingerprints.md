# [WIP] fix(checker): align let/var redeclaration fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/let-var-redeclaration-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/compiler/letAndVarRedeclaration.ts`, where `tsc`
and `tsz` emit the same diagnostic codes (`TS2300`, `TS2451`) but the
fingerprints differ.

## Files Touched

- `docs/plan/claims/fix-let-var-redeclaration-fingerprints.md`

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "letAndVarRedeclaration" --verbose`
- Focused Rust regression test in the owning crate
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
