# fix(checker): align nodeModulesJson diagnostic fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-node-modules-json-fingerprints`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance - diagnostic fingerprints)

## Intent

The canonical conformance picker selected
`TypeScript/tests/cases/conformance/node/nodeModulesJson.ts`, currently a
fingerprint-only failure: `tsz` emits the same diagnostic code set as `tsc`
(`TS1544`, `TS2339`, `TS2823`) but differs in one or more diagnostic
fingerprints.

This PR will inspect the message/anchor/display divergence, fix the root cause
in the appropriate checker/solver/printer layer, and add a focused Rust
regression test for the invariant.

## Files Touched

- TBD after implementation.

## Verification

- `cargo check --package tsz-checker`
- focused Rust regression test in the owning crate
- `./scripts/conformance/conformance.sh run --filter "nodeModulesJson" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
