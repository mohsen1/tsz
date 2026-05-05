# fix(checker): align nodeModules1 diagnostic fingerprints

- **Date**: 2026-05-05 19:53:59 UTC
- **Branch**: `fix/checker-node-modules1-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance - diagnostic fingerprints)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/conformance/node/nodeModules1.ts`, currently a
fingerprint-only failure: `tsz` emits the same diagnostic code set as `tsc`
(`TS1471`, `TS1479`, `TS2307`, `TS2834`, `TS2835`) but differs in one or more
diagnostic fingerprints.

This PR will inspect the message/anchor/display divergence, fix the root cause
in the appropriate resolver/checker/printer layer, and add a focused Rust
regression test for the invariant.

## Files Touched

- TBD after implementation.

## Verification

- `cargo check --package tsz-checker`
- focused Rust regression test in the owning crate
- `./scripts/conformance/conformance.sh run --filter "nodeModules1" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
