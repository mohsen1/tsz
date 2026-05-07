# [WIP] fix(checker): align required mapped type variance diagnostics

- **Date**: 2026-05-07
- **Branch**: `fix/conformance-next-20260507-063831`
- **PR**: #4341
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the remaining fingerprint-only conformance failure for
`TypeScript/tests/cases/compiler/requiredMappedTypeModifierTrumpsVariance.ts`.
`tsc` and `tsz` already agree on the diagnostic codes (`TS2322`, `TS2339`,
`TS2741`), but one or more diagnostic fingerprints still differ. An older
ready claim fixed the TS2339 receiver display for this fixture; this slice is
scoped to the remaining mismatch.

## Files Touched

- TBD after diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "requiredMappedTypeModifierTrumpsVariance" --verbose`
- Planned: focused Rust regression test in the owning crate
- Planned: `./scripts/conformance/conformance.sh snapshot`
