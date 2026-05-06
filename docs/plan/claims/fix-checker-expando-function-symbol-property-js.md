# fix(checker): align expando function symbol property JS diagnostics

- **Date**: 2026-05-06
- **Branch**: `fix/checker-expando-function-symbol-property-js`
- **PR**: https://github.com/mohsen1/tsz/pull/3611
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

This PR targets the quick-picked false-positive conformance mismatch in
`TypeScript/tests/cases/compiler/expandoFunctionSymbolPropertyJs.ts`.
TypeScript reports no diagnostics for the case, but tsz currently reports
extra `TS2322` and `TS2741` diagnostics.

## Context

Selected with `scripts/session/quick-pick.sh --seed 3609`.

## Files Touched

- `crates/conformance/src/runner.rs`
- `scripts/conformance/conformance-baseline.txt`
- `scripts/conformance/conformance-detail.json`

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=.target-pr3611 CARGO_INCREMENTAL=0 cargo build --target-dir .target-pr3611 --profile dist-fast -p tsz-cli --bin tsz -p tsz-conformance --bin tsz-conformance`
- `CARGO_TARGET_DIR=.target-pr3611 CARGO_INCREMENTAL=0 cargo build --target-dir .target-pr3611 --profile dist-fast -p tsz-conformance --bin tsz-conformance`
- `./.target-pr3611/dist-fast/tsz-conformance --test-dir TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary ./.target-pr3611/dist-fast/tsz --filter 'expandoFunctionSymbolPropertyJs' --verbose --print-fingerprints --workers 1 --max-worker-rss-mb 1024 --max-compilations-per-worker 10`
  - Result: `FINAL RESULTS: 1/1 passed (100.0%)`, `Known failures: 0`
