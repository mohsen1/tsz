# [WIP] fix(checker): align JS initializer diagnostic fingerprints

- **Date**: 2026-04-29
- **Timestamp**: 2026-04-29 21:53:00 UTC
- **Branch**: `fix/checker-js-initializer-fingerprint`
- **PR**: #1830
- **Status**: claim
- **Workstream**: 1 - Diagnostic Conformance And Fingerprints

## Intent

Picked by `scripts/session/quick-pick.sh` on 2026-04-29. The target
`TypeScript/tests/cases/conformance/salsa/typeFromJSInitializer.ts` is a
fingerprint-only mismatch with matching diagnostic codes (`TS2322`, `TS7006`,
and `TS7008`) but divergent fingerprint details. This PR will identify the
root cause, fix the owning checker/solver/printer path, and add a focused Rust
regression test for the invariant.

## Files Touched

- `docs/plan/claims/fix-checker-js-initializer-fingerprint.md`

## Verification

- Planned: `cargo check --package tsz-checker`
- Planned: `cargo check --package tsz-solver`
- Planned: `cargo build --profile dist-fast --bin tsz`
- Planned: targeted unit tests for the owning crate
- Planned: `./scripts/conformance/conformance.sh run --filter "typeFromJSInitializer" --verbose`
- Planned: `./scripts/conformance/conformance.sh run --max 200`
- Planned: `scripts/safe-run.sh ./scripts/conformance/conformance.sh run`
