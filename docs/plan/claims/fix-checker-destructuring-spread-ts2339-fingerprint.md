# fix(checker): align destructuring spread TS2339 fingerprint

- **Date**: 2026-04-29
- **Branch**: `fix/checker-destructuring-spread-ts2339-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Fix the fingerprint-only TS2339 conformance mismatch for
`TypeScript/tests/cases/conformance/es6/destructuring/destructuringSpread.ts`.
The slice will diagnose why tsz emits the right diagnostic code but a
different diagnostic fingerprint than `tsc`, then adjust the owning checker,
solver boundary, or diagnostic rendering path without adding a local
suppression.

## Files Touched

- `docs/plan/claims/fix-checker-destructuring-spread-ts2339-fingerprint.md`
- Implementation files TBD after verbose conformance investigation.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "destructuringSpread" --verbose`
- Planned: targeted `cargo nextest run` for the owning crate tests changed.
