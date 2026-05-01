# [WIP] fix(checker): align prototype assignment TS2339 fingerprint

- **Date**: 2026-05-01
- **Branch**: `fix/checker-type-from-prototype-assignment-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Investigate and fix the fingerprint-only TS2339 mismatch in
`TypeScript/tests/cases/conformance/salsa/typeFromPrototypeAssignment.ts`.
The work will preserve the shared property-access diagnostic path and add an
owning-crate regression test for the structural rule behind the mismatch.

## Files Touched

- `docs/plan/claims/fix-checker-type-from-prototype-assignment-fingerprint.md`
- Implementation files TBD after reproducing and localizing the mismatch.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "typeFromPrototypeAssignment" --verbose`
- Planned: targeted `cargo nextest run` for the owning crate test.
