# [WIP] fix(checker): align call-signature subtype diagnostics

- **Date**: 2026-05-07
- **Branch**: `fix/conformance-next-20260507-061223`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the fingerprint-only conformance failure for
`TypeScript/tests/cases/conformance/types/typeRelationships/subtypesAndSuperTypes/subtypingWithCallSignatures3.ts`.
`tsc` and `tsz` already agree on the diagnostic codes (`TS2352`, `TS2564`),
but differ in one or more diagnostic fingerprints. The intended scope is a
small checker/solver/printer fix for the root display or anchoring mismatch,
with focused Rust regression coverage and refreshed conformance snapshots.

## Files Touched

- TBD after diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "subtypingWithCallSignatures3" --verbose`
- Planned: focused Rust regression test in the owning crate
- Planned: `scripts/conformance/conformance.sh snapshot --force`
