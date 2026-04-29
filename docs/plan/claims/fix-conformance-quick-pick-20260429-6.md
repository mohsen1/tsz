# [WIP] fix(checker): suppress extra TS2532 for recursive implicit constructor

- **Date**: 2026-04-29
- **Branch**: `fix/conformance-quick-pick-20260429-6`
- **PR**: #1815
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

The conformance picker selected `TypeScript/tests/cases/compiler/genericRecursiveImplicitConstructorErrors3.ts`.
`tsc` reports `TS2314` and `TS2339`, while `tsz` additionally reports `TS2532`.
This PR will diagnose the extra nullish-object diagnostic and fix the root cause in the appropriate checker/solver boundary.

## Files Touched

- `docs/plan/claims/fix-conformance-quick-pick-20260429-6.md`
- Implementation files TBD after diagnosis.

## Verification

- Planned: `cargo check --package tsz-checker`
- Planned: `cargo check --package tsz-solver`
- Planned: `cargo build --profile dist-fast --bin tsz`
- Planned: `cargo nextest run --package tsz-checker --lib`
- Planned: targeted owning-crate unit tests for the fix
- Planned: `./scripts/conformance/conformance.sh run --filter "genericRecursiveImplicitConstructorErrors3" --verbose`
- Planned: `./scripts/conformance/conformance.sh run --max 200`
