# [WIP] fix(checker): align shorthand destructuring diagnostics

- **Date**: 2026-04-28
- **Branch**: `fix/checker-destructuring-shorthand-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance / fingerprint-only)

## Intent

Fix the fingerprint-only conformance mismatch in
`shorthandPropertyAssignmentsInDestructuring_ES6.ts`, where tsz reports the
same TS1312 and TS2322 codes as tsc but with divergent diagnostic fingerprints.
The slice will diagnose whether the mismatch is display, anchor, or elaboration
policy and route the fix through the appropriate checker/solver boundary.

## Files Touched

- `docs/plan/claims/fix-checker-destructuring-shorthand-fingerprint.md`

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "shorthandPropertyAssignmentsInDestructuring_ES6" --verbose`
- Planned: targeted unit tests in the owning crate
- Planned: relevant `cargo nextest run` package filters for touched crates
