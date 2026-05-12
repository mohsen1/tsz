# fix(checker): align recursiveTypeReferences1 TS2322 fingerprints

- **Date**: 2026-05-12
- **Branch**: `fix/recursive-type-references1-display-20260512`
- **PR**: https://github.com/mohsen1/tsz/pull/5703
- **Status**: ready
- **Workstream**: conformance

## Intent

Continue the `recursiveTypeReferences1.ts` conformance work after prior slices removed extra diagnostic codes. This slice will inspect the current fingerprint-only TS2322 drift and fix the smallest display or anchor mismatch that can be owned by checker diagnostics without broad recursive-type semantic changes.

## Files Touched

- `docs/plan/claims/fix-recursive-type-references1-display-20260512.md`
- `crates/tsz-checker/src/state/state_checking/source_file.rs`

## Verification

- Baseline: `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter recursiveTypeReferences1 --print-fingerprints --verbose` (0/1 passed; fingerprint-only drift included `Box<number | Box2>` instead of `Box2`)
- `cargo build --profile dist-fast -p tsz-cli -p tsz-conformance` (passed)
- `.target/dist-fast/tsz-conformance --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target/dist-fast/tsz --filter recursiveTypeReferences1 --print-fingerprints --verbose` (0/1 passed; `Box2` TS2322 fingerprint pair removed; remaining drift is nested array anchoring/message expansion)
