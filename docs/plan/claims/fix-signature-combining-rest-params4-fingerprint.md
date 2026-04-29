# [WIP] fix(checker): align rest-parameter union TS2345 fingerprint

- **Date**: 2026-04-29
- **Branch**: `fix/signature-combining-rest-params4-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

`scripts/session/quick-pick.sh` selected `signatureCombiningRestParameters4.ts`, a fingerprint-only TS2345 mismatch. This PR will diagnose why the assignability diagnostic for a call through `RemoveThis<AnyConfig["extendMarkSchema"]>` differs from `tsc`, then fix the owning checker/solver/printer path with a focused regression test.

## Files Touched

- `docs/plan/claims/fix-signature-combining-rest-params4-fingerprint.md` (claim/status)
- Implementation files TBD after diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "signatureCombiningRestParameters4" --verbose`
- Planned: unit tests for the owning crate changed by the fix.
