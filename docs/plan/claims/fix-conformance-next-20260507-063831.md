# fix(checker): align required mapped type variance diagnostics

- **Date**: 2026-05-07
- **Branch**: `fix/conformance-next-20260507-063831`
- **PR**: #4341
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the remaining fingerprint-only conformance failure for
`TypeScript/tests/cases/compiler/requiredMappedTypeModifierTrumpsVariance.ts`.
`tsc` and `tsz` already agree on the diagnostic codes (`TS2322`, `TS2339`,
`TS2741`), but one or more diagnostic fingerprints still differ. An older
ready claim fixed the TS2339 receiver display for this fixture; this slice is
scoped to the remaining mismatch.

## Files Touched

- `crates/tsz-checker/src/types/computation/object_literal_context.rs`
- `crates/tsz-checker/src/tests/dispatch_tests.rs`

## Verification

- `cargo nextest run -p tsz-checker nested_mapped_application_property_preserves_literal_context`
- `./scripts/conformance/conformance.sh run --filter "requiredMappedTypeModifierTrumpsVariance" --verbose`
- `python3 scripts/conformance/check-snapshot-regression.py --base-ref origin/main --head-ref HEAD`

The broad generated snapshot refresh was intentionally dropped from this PR
because it introduced unrelated new failing paths even though the targeted
fixture now passes. The snapshot gate should stay responsible for blocking that
kind of churn.
