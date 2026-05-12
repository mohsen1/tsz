# fix(checker): report merged Baz variance assignment

- **Date**: 2026-05-12
- **Branch**: `fix/variance-annotations-baz-20260512`
- **Base**: `fix/variance-annotations-anon-class-20260512`
- **PR**: https://github.com/mohsen1/tsz/pull/5714
- **Status**: ready
- **Workstream**: conformance

## Intent

Continue the `varianceAnnotations.ts` conformance cleanup by targeting the remaining missing diagnostic on the merged `Baz` interface assignment:

- `baz1 = baz2;` should report `TS2322 test.ts:117:1 Type 'Baz<string>' is not assignable to type 'Baz<unknown>'.`

This required semantic handling of conflicting variance annotations across merged interface declarations, not another display-only cleanup.

## Files Touched

- `crates/tsz-checker/src/context/resolver.rs`
- `docs/plan/claims/fix-variance-annotations-baz-20260512.md`

## Verification

- Baseline on the stacked branch before this slice: focused `varianceAnnotations` reported `1/2 passed`, fingerprint-only `1`, with only the missing `TS2322 test.ts:117:1 Type 'Baz<string>' is not assignable to type 'Baz<unknown>'.`
- `cargo fmt --all && git diff --check` passed.
- `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance` passed in 42.75s.
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter varianceAnnotations --print-fingerprints --verbose` reports `2/2 passed`, fingerprint-only `0`.
