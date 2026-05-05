# [WIP] fix(checker): align generic optional fold fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/generic-optional-fold-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/compiler/genericFunctionsWithOptionalParameters2.ts`,
where `tsc` and `tsz` emit the same diagnostic codes (`TS2345`, `TS2554`) but
the fingerprints differ.

## Files Touched

- `docs/plan/claims/fix-generic-optional-fold-fingerprints.md`

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "genericFunctionsWithOptionalParameters2" --verbose`
- Focused Rust regression test in the owning crate
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
