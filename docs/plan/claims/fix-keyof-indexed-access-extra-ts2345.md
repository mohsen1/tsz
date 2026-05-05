# [WIP] fix(checker): suppress extra TS2345 in keyof indexed access

- **Date**: 2026-05-05
- **Branch**: `fix/keyof-indexed-access-extra-ts2345`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the random conformance pick
`TypeScript/tests/cases/conformance/types/keyof/keyofAndIndexedAccess.ts`,
where `tsc` expects only `TS2322` but `tsz` currently emits an extra `TS2345`.

## Files Touched

- `docs/plan/claims/fix-keyof-indexed-access-extra-ts2345.md`

## Verification Plan

- `./scripts/conformance/conformance.sh run --filter "keyofAndIndexedAccess" --verbose`
- Focused Rust regression test in the owning crate
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/githooks/pre-commit`
