# fix(checker): align nodeModulesJson diagnostic fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/checker-node-modules-json-fingerprints`
- **PR**: #3201
- **Status**: implemented
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

- `crates/tsz-checker/src/state/type_analysis/computed_commonjs/exports_detection.rs`
- `crates/tsz-checker/tests/json_namespace_import_tests.rs`
- `crates/tsz-checker/Cargo.toml`
- `docs/plan/claims/fix-checker-node-modules-json-fingerprints.md`

## Verification

- `cargo fmt --check`
- `cargo nextest run -p tsz-checker --test json_namespace_import_tests --no-fail-fast`
- `cargo check --workspace`
- `./scripts/conformance/conformance.sh run --filter "nodeModulesJson" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --filter "nodeModulesResolveJsonModule" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run`:
  - `FINAL RESULTS: 12468/12582 passed (99.1%)`
  - `Crashed: 1`, `Timeout: 1`, `Fingerprint-only: 67`
  - `Net: 12453 -> 12468 (+15)`
  - `TypeScript/tests/cases/conformance/node/nodeModulesJson.ts` listed under improvements
