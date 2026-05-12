# fix(checker): align variadicTuples1 fingerprints

- **Date**: 2026-05-12
- **Branch**: `fix/variadic-tuples1-fingerprints-20260512`
- **Base**: `fix/conditional-types1-fingerprints-20260512`
- **PR**: https://github.com/mohsen1/tsz/pull/5730
- **Status**: ready
- **Workstream**: conformance

## Intent

Reduce the remaining non-XFAIL conformance fingerprint-only failures after the current `conditionalTypes1` slice. This slice targets `variadicTuples1.ts`, which is the other remaining non-XFAIL mismatch in the local full conformance snapshot.

## Files Touched

- `docs/plan/claims/fix-variadic-tuples1-fingerprints-20260512.md`
- `crates/tsz-checker/src/state/state_checking/source_file.rs`

## Verification

- Baseline: full local conformance snapshot after the index-signatures slice reported only `conditionalTypes1` and `variadicTuples1` as non-XFAIL fingerprint-only failures.
- `cargo fmt --all`
- `git diff --check`
- `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance`
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file /Users/mohsen/code/tsz/scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter variadicTuples1 --print-fingerprints --verbose`
  - Result: `1/1 passed (100.0%)`, `Fingerprint-only: 0`
