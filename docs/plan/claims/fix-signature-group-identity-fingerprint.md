# [WIP] fix(checker): align signature group identity diagnostics

- **Date**: 2026-04-29
- **Branch**: `fix/signature-group-identity-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Investigate the fingerprint-only conformance failure in
`orderMattersForSignatureGroupIdentity.ts`. The PR will align TSZ's diagnostic
fingerprints with `tsc` for the signature group identity case, fixing the
owning checker/solver/printer invariant rather than adding a local suppression.

## Files Touched

- `docs/plan/claims/fix-signature-group-identity-fingerprint.md`
- Implementation files TBD after diagnosis.

## Verification

- Planned: `./scripts/conformance/conformance.sh run --filter "orderMattersForSignatureGroupIdentity" --verbose`
- Planned: owning crate unit tests via `cargo nextest run`
