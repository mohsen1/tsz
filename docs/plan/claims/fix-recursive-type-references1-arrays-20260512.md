# fix(checker): align recursiveTypeReferences1 array diagnostics

- **Date**: 2026-05-12
- **Branch**: `fix/recursive-type-references1-arrays-20260512`
- **PR**: https://github.com/mohsen1/tsz/pull/5712
- **Status**: ready
- **Workstream**: conformance

## Intent

Continue the `recursiveTypeReferences1.ts` conformance cleanup after the merged Box2 display fix. This slice targets the remaining nested recursive array diagnostic anchoring/message drift around `RecArray<T>` and recursive array aliases.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `docs/plan/claims/fix-recursive-type-references1-arrays-20260512.md`

## Verification

- Baseline on current `origin/main`: focused `recursiveTypeReferences1` reported `0/1 passed`, fingerprint-only `1`, with three missing TSC array diagnostics and multiple extra nested array-element TS2322 diagnostics.
- `cargo fmt --all && git diff --check` passed.
- `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance` passed in 58.20s.
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter recursiveTypeReferences1 --print-fingerprints --verbose` reports `1/1 passed`, fingerprint-only `0`.
