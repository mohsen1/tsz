# [WIP] fix(checker): align template literal conformance fingerprints

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-20260505-next13`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the picked `templateLiteralTypes1.ts` fingerprint mismatch by addressing the
root cause behind misplaced template-literal complexity diagnostics and extra
template-literal assignability errors. The initial target has matching error
codes but divergent fingerprints, so the work will separate semantic relation
bugs from diagnostic rendering/anchoring bugs before changing behavior.

## Files Touched

- TBD after root-cause investigation

## Verification

- `./scripts/conformance/conformance.sh run --filter "templateLiteralTypes1" --verbose`
- focused Rust unit tests in the owning crate
- broader conformance regression check before marking ready
