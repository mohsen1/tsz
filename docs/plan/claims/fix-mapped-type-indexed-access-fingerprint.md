# [WIP] fix(checker): align mapped indexed access TS2322 fingerprint

- **Date**: 2026-04-29
- **Branch**: `fix/mapped-type-indexed-access-fingerprint`
- **PR**: #1816
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

`scripts/session/quick-pick.sh` selected `TypeScript/tests/cases/compiler/mappedTypeIndexedAccess.ts`, a fingerprint-only TS2322 mismatch. This PR will diagnose the root cause of the message or anchor divergence and align tsz with tsc through the shared checker/solver diagnostic paths without adding checker-local semantic shortcuts.

## Files Touched

- `docs/plan/claims/fix-mapped-type-indexed-access-fingerprint.md`

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "mappedTypeIndexedAccess" --verbose`
- Planned: owning-crate unit tests for the changed invariant
