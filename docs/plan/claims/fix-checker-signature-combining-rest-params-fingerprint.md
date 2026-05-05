# [WIP] fix(checker): align signature-combining rest parameter fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix/checker-signature-combining-rest-params-fingerprint`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance / diagnostic fingerprints)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/compiler/signatureCombiningRestParameters4.ts`, a
fingerprint-only failure where tsz and tsc agree on diagnostic code `TS2345`
but disagree on diagnostic fingerprint details.

This PR will root-cause the rest-parameter signature-combining display or
anchor mismatch, add owning Rust regression coverage, and rerun the targeted
conformance test.

## Files Touched

- TBD after root-cause analysis.

## Verification

- Baseline target command:
  `./scripts/conformance/conformance.sh run --filter "signatureCombiningRestParameters4" --verbose`
- Planned: owning-crate Rust regression test.
- Planned: targeted conformance rerun for `signatureCombiningRestParameters4`.
