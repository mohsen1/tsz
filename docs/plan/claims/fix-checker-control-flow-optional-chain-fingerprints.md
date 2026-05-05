# fix(checker): align control-flow optional-chain fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-control-flow-optional-chain-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the fingerprint-only conformance drift in
`TypeScript/tests/cases/conformance/controlFlow/controlFlowOptionalChain.ts`.
The error-code set already matches `tsc` (`TS2454`, `TS2722`, `TS18048`), so
this slice is expected to focus on diagnostic anchoring or message rendering
for optional-chain control-flow errors.

## Files Touched

- TBD

## Verification

- `cargo fmt --all --check`
- `cargo check --package tsz-checker --package tsz-solver`
- Targeted checker regression test
- `./scripts/conformance/conformance.sh run --filter "controlFlowOptionalChain" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
