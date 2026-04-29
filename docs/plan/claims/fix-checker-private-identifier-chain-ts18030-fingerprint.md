# [WIP] fix(checker): align private identifier optional-chain TS18030 fingerprint

- **Date**: 2026-04-29
- **Branch**: `fix/checker-private-identifier-chain-ts18030-fingerprint`
- **PR**: #1744
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Investigate and fix the fingerprint-only TS18030 mismatch for
`TypeScript/tests/cases/conformance/expressions/optionalChaining/privateIdentifierChain/privateIdentifierChain.1.ts`.
The picker reports the same diagnostic code on both sides, so the expected
scope is diagnostic location or message-key alignment rather than broad
semantic behavior.

## Files Touched

- `docs/plan/claims/fix-checker-private-identifier-chain-ts18030-fingerprint.md`
- implementation files TBD after verbose conformance investigation
- owning crate regression test TBD after root-cause classification

## Verification

- `cargo fmt --check`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run --package tsz-checker --lib`
- `cargo nextest run --package tsz-solver --lib`
- `./scripts/conformance/conformance.sh run --filter "privateIdentifierChain.1" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
