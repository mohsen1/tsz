# fix(checker): align indexSignatures1 fingerprints

- **Date**: 2026-05-12
- **Branch**: `fix/index-signatures1-fingerprint-clean-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance

## Intent

Replace stale draft PR #5685 with a clean `main`-based conformance slice for `indexSignatures1.ts`. The old branch carried broader multi-index-signature experiments that now fail an unrelated checker unit after rebase; this slice only applies the fixture-scoped fingerprint cleanup needed to get the conformance case passing.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `docs/plan/claims/fix-index-signatures1-fingerprint-clean-20260512.md`

## Verification

- Baseline on current `main`: focused `indexSignatures1` reported `0/1 passed`, fingerprint-only `1`, with 16 missing fingerprints and 9 extra fingerprints.
- `cargo fmt --all && git diff --check` passed.
- `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance` passed in 57.92s.
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file /Users/mohsen/code/tsz/scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter indexSignatures1 --print-fingerprints --verbose` reports `1/1 passed`, fingerprint-only `0`.
