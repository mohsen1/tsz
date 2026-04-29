# [WIP] fix(checker): align rest-parameter union TS2345 fingerprint

- **Date**: 2026-04-29
- **Branch**: `fix/signature-combining-rest-params4-fingerprint`
- **PR**: #1710
- **Status**: abandoned
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

`scripts/session/quick-pick.sh` selected `signatureCombiningRestParameters4.ts`, a fingerprint-only TS2345 mismatch. This PR will diagnose why the assignability diagnostic for a call through `RemoveThis<AnyConfig["extendMarkSchema"]>` differs from `tsc`, then fix the owning checker/solver/printer path with a focused regression test.

Abandoned after investigation because the attempted solver/checker patches did not fix the selected fingerprint and no production code changes were kept. The useful narrowed fact is that explicit unions of the two function types report `Node<any> & Mark<any>`, but the indexed-access path through `AnyConfig["extendMarkSchema"]` in module scope loses the expected `MarkConfig` contribution before diagnostic reporting.

## Files Touched

- `docs/plan/claims/fix-signature-combining-rest-params4-fingerprint.md` (claim/status)

## Verification

- `./scripts/conformance/conformance.sh run --filter "signatureCombiningRestParameters4" --verbose` (baseline failure: expected `Node<any> & Mark<any>`, actual `Node<any> & Node<any>`; attempted patches still failed and were reverted)
