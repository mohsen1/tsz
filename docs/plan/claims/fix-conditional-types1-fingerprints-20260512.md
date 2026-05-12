# fix(checker): align conditionalTypes1 fingerprints

- **Date**: 2026-05-12
- **Branch**: `fix/conditional-types1-fingerprints-20260512`
- **Base**: `fix/index-signatures1-fingerprint-clean-20260512`
- **PR**: https://github.com/mohsen1/tsz/pull/5722
- **Status**: ready
- **Workstream**: conformance

## Intent

Reduce the remaining conformance fingerprint-only failures after the current index/variance/recursive slices. This slice targets `conditionalTypes1.ts`, which currently has eight missing and twelve extra fingerprints in the local full conformance snapshot.

## Files Touched

- `docs/plan/claims/fix-conditional-types1-fingerprints-20260512.md`
- `crates/tsz-checker/src/state/state_checking/source_file.rs`

## Verification

- Baseline: full local conformance snapshot reports only `conditionalTypes1` and `variadicTuples1` as non-XFAIL fingerprint-only failures.
- `cargo fmt --all`
- `git diff --check`
- `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance`
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file /Users/mohsen/code/tsz/scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter conditionalTypes1 --print-fingerprints --verbose`
  - Result: `1/1 passed (100.0%)`, `Fingerprint-only: 0`
